# 認証フロー図

## 1. ユーザー登録フロー

```
┌─────────┐                ┌─────────────┐              ┌──────────────┐
│ Client  │                │  Auth API   │              │  Repository  │
└────┬────┘                └──────┬──────┘              └──────┬───────┘
     │                            │                            │
     │  POST /auth/register       │                            │
     │  (username, password)      │                            │
     ├───────────────────────────>│                            │
     │                            │                            │
     │                            │  validate_password()       │
     │                            ├─────────┐                  │
     │                            │         │                  │
     │                            │<────────┘                  │
     │                            │                            │
     │                            │  hash_password(Argon2)     │
     │                            ├─────────┐                  │
     │                            │         │                  │
     │                            │<────────┘                  │
     │                            │                            │
     │                            │  create_user()             │
     │                            ├───────────────────────────>│
     │                            │                            │
     │                            │                  (PostgreSQL)
     │                            │                  INSERT user
     │                            │                            │
     │                            │<───────────────────────────┤
     │                            │                            │
     │   201 Created              │                            │
     │   (user_id, username)      │                            │
     │<───────────────────────────┤                            │
     │                            │                            │
```

## 2. ログインフロー（JWT + セッション）

```
┌─────────┐      ┌─────────────┐      ┌──────────────┐      ┌─────────┐
│ Client  │      │  Auth API   │      │  Repository  │      │  Redis  │
└────┬────┘      └──────┬──────┘      └──────┬───────┘      └────┬────┘
     │                  │                    │                    │
     │  POST /auth/login                     │                    │
     │  (email, password)                    │                    │
     ├─────────────────>│                    │                    │
     │                  │                    │                    │
     │                  │  find_by_email()   │                    │
     │                  ├───────────────────>│                    │
     │                  │                    │                    │
     │                  │<───────────────────┤                    │
     │                  │  (user + password_hash)                 │
     │                  │                    │                    │
     │                  │  verify_password(Argon2)                │
     │                  ├─────────┐          │                    │
     │                  │         │          │                    │
     │                  │<────────┘          │                    │
     │                  │                    │                    │
     │                  │  generate_jwt()    │                    │
     │                  ├─────────┐          │                    │
     │                  │         │          │                    │
     │                  │<────────┘          │                    │
     │                  │  (access_token, refresh_token)          │
     │                  │                    │                    │
     │                  │  create_session()  │                    │
     │                  │                    │                    │
     │                  ├───────────────────────────────────────>│
     │                  │                                  (Redis)│
     │                  │                               SET session│
     │                  │                               EXPIRE TTL │
     │                  │<───────────────────────────────────────┤
     │                  │                    │                    │
     │   200 OK         │                    │                    │
     │   {              │                    │                    │
     │     access_token,│                    │                    │
     │     refresh_token│                    │                    │
     │     user_info    │                    │                    │
     │   }              │                    │                    │
     │<─────────────────┤                    │                    │
     │                  │                    │                    │
```

## 3. 保護されたエンドポイントアクセスフロー

```
┌─────────┐      ┌───────────────┐      ┌──────────────┐      ┌─────────┐
│ Client  │      │  Middleware   │      │   Handler    │      │  Redis  │
└────┬────┘      └───────┬───────┘      └──────┬───────┘      └────┬────┘
     │                   │                     │                    │
     │  GET /api/user/me │                     │                    │
     │  Authorization: Bearer <token>          │                    │
     ├──────────────────>│                     │                    │
     │                   │                     │                    │
     │                   │  extract_token()    │                    │
     │                   ├─────────┐           │                    │
     │                   │         │           │                    │
     │                   │<────────┘           │                    │
     │                   │                     │                    │
     │                   │  verify_jwt()       │                    │
     │                   ├─────────┐           │                    │
     │                   │         │           │                    │
     │                   │<────────┘           │                    │
     │                   │  (claims)           │                    │
     │                   │                     │                    │
     │                   │  get_session()      │                    │
     │                   ├───────────────────────────────────────>│
     │                   │                               (Optional)│
     │                   │                                  GET key│
     │                   │<───────────────────────────────────────┤
     │                   │                     │                    │
     │                   │  set_user_in_extensions()               │
     │                   ├─────────┐           │                    │
     │                   │         │           │                    │
     │                   │<────────┘           │                    │
     │                   │                     │                    │
     │                   │  next.run(request)  │                    │
     │                   ├────────────────────>│                    │
     │                   │                     │                    │
     │                   │                     │  get_user_from_extensions()
     │                   │                     ├─────────┐          │
     │                   │                     │         │          │
     │                   │                     │<────────┘          │
     │                   │                     │                    │
     │                   │                     │  process()         │
     │                   │                     ├─────────┐          │
     │                   │                     │         │          │
     │                   │                     │<────────┘          │
     │                   │                     │                    │
     │                   │   200 OK            │                    │
     │                   │<────────────────────┤                    │
     │                   │                     │                    │
     │   200 OK          │                     │                    │
     │   (user data)     │                     │                    │
     │<──────────────────┤                     │                    │
     │                   │                     │                    │
```

## 4. トークンリフレッシュフロー

```
┌─────────┐                ┌─────────────┐              ┌──────────────┐
│ Client  │                │  Auth API   │              │  Repository  │
└────┬────┘                └──────┬──────┘              └──────┬───────┘
     │                            │                            │
     │  POST /auth/refresh        │                            │
     │  (refresh_token)           │                            │
     ├───────────────────────────>│                            │
     │                            │                            │
     │                            │  verify_refresh_token()    │
     │                            ├─────────┐                  │
     │                            │         │                  │
     │                            │<────────┘                  │
     │                            │  (claims)                  │
     │                            │                            │
     │                            │  find_by_id(claims.sub)    │
     │                            ├───────────────────────────>│
     │                            │                            │
     │                            │<───────────────────────────┤
     │                            │  (user)                    │
     │                            │                            │
     │                            │  generate_token_pair()     │
     │                            ├─────────┐                  │
     │                            │         │                  │
     │                            │<────────┘                  │
     │                            │  (new tokens)              │
     │                            │                            │
     │   200 OK                   │                            │
     │   (access_token, refresh_token)                         │
     │<───────────────────────────┤                            │
     │                            │                            │
```

## 5. ロールベースアクセス制御フロー

```
┌─────────┐      ┌───────────────┐      ┌──────────────┐
│ Client  │      │  Middleware   │      │   Handler    │
└────┬────┘      └───────┬───────┘      └──────┬───────┘
     │                   │                     │
     │  GET /api/admin/stats                   │
     │  Authorization: Bearer <token>          │
     ├──────────────────>│                     │
     │                   │                     │
     │                   │  verify_jwt()       │
     │                   ├─────────┐           │
     │                   │         │           │
     │                   │<────────┘           │
     │                   │  (user)             │
     │                   │                     │
     │                   │  check_role(Admin)  │
     │                   ├─────────┐           │
     │                   │         │           │
     │                   │<────────┘           │
     │                   │                     │
     │                   │  ┌──────────────────────┐
     │                   │  │ user.roles.contains  │
     │                   │  │      (Admin)?        │
     │                   │  └──────────────────────┘
     │                   │          │
     │                   │         Yes
     │                   │          │
     │                   │          ▼
     │                   │  set_user_in_extensions()
     │                   │                     │
     │                   │  next.run(request)  │
     │                   ├────────────────────>│
     │                   │                     │
     │   200 OK          │   Admin Data        │
     │<──────────────────┤<────────────────────┤
     │                   │                     │
     │                   │                     │
     │   (If No: 403 Forbidden)                │
     │                   │                     │
```

## データストレージ

### PostgreSQL (ユーザーデータ)

```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    email TEXT UNIQUE,
    roles TEXT NOT NULL,         -- JSON array
    permissions TEXT NOT NULL,   -- JSON array
    provider TEXT NOT NULL,
    metadata TEXT NOT NULL,      -- JSON object
    password_hash TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);
```

### Redis (セッション)

```
Key Format: session:<session_id>
Value: JSON {
    user: AuthUser,
    created_at: timestamp,
    last_accessed_at: timestamp,
    metadata: HashMap
}
TTL: 3600 seconds (1 hour) or 2592000 seconds (30 days with remember_me)
```

## セキュリティレイヤー

```
┌─────────────────────────────────────────────┐
│           Application Layer                 │
├─────────────────────────────────────────────┤
│  Authorization (Role/Permission Check)      │
├─────────────────────────────────────────────┤
│  Authentication (JWT Verification)          │
├─────────────────────────────────────────────┤
│  Password Hashing (Argon2, cost=12)         │
├─────────────────────────────────────────────┤
│  TLS/HTTPS (Transport Security)             │
└─────────────────────────────────────────────┘
```

## エラーハンドリングフロー

```
┌─────────┐                ┌─────────────┐
│ Client  │                │  Auth API   │
└────┬────┘                └──────┬──────┘
     │                            │
     │  POST /auth/login          │
     │  (invalid credentials)     │
     ├───────────────────────────>│
     │                            │
     │                            │  authenticate()
     │                            ├─────────┐
     │                            │  FAIL   │
     │                            │<────────┘
     │                            │
     │   401 Unauthorized         │
     │   {                        │
     │     "error": "InvalidCredentials",
     │     "message": "Invalid email or password"
     │   }                        │
     │<───────────────────────────┤
     │                            │
```

## トークンライフサイクル

```
時間軸:
0分                                    60分                    1440分
|--------------------------------------|------------------------|
        Access Token有効期間                Refresh Token有効期間
        (1時間)                               (24時間)

■ Access Token: 短期間有効、API呼び出しに使用
■ Refresh Token: 長期間有効、Access Token再発行に使用

リフレッシュ戦略:
- Access Token期限切れ前にRefreshエンドポイントで更新
- Refresh Tokenも同時に再発行（トークンローテーション）
- 古いRefresh Tokenは無効化（将来実装）
```
