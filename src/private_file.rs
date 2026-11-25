//! 本人だけが読み書きできるファイルを、作るときから制限して作る（#599・#674）。
//!
//! Unixでは、モード`600`で作り、実際のモードを確かめる（#684）。Windowsでは、本人とSYSTEM・Administrators
//! だけが使えるACL（親フォルダから受け継がない）を付けて作る。書いた後に
//! 制限すると、その間にほかの人に読まれ得るため、作るときに渡す。
//!
//! 設定ファイルには、前の設定から写した平文のパスワードが残ることがあり、
//! 控えには前の設定ファイルがそのまま入る。`--config`で、ほかの人も読める
//! フォルダに書くときも、ほかのローカルユーザーに読まれないようにする。

use std::fs::File;
use std::io;
use std::path::Path;

/// `path`に、本人だけが読み書きできるファイルを新しく作る（既にあれば
/// `AlreadyExists`のエラー）。書き込みのために開いたファイルを返す
pub(crate) fn create_new_private(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        // 実際のモードを、開いたファイルで確かめる（#684）。vfat等では、
        // マウントの`fmask`で決まり、ほかの人も読めることがある。中身を
        // 書く前に、作ったファイルを消して止める
        let checked = file.metadata().and_then(|metadata| {
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 == 0 {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "cannot make {} readable by you only: the file system gave it mode {mode:o} instead of 600 (it does not keep file permissions, such as FAT32 or exFAT, or it is mounted that way). Use a folder on another file system",
                    path.display()
                )))
            }
        });
        if let Err(error) = checked {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(file)
    }
    #[cfg(windows)]
    {
        windows::create_new_private(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    }
}

#[cfg(windows)]
mod windows {
    use std::fs::File;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::Path;

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorControl, GetTokenInformation, TokenUser, ACL,
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetVolumeInformationByHandleW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// 終わるときに`LocalFree`で放す
    struct LocalBox<T>(*mut T);

    impl<T> Drop for LocalBox<T> {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: Windowsが`LocalAlloc`で確保して渡したもので、1度だけ放す
                unsafe { LocalFree(self.0.cast()) };
            }
        }
    }

    /// 終わるときに`CloseHandle`で閉じる
    struct OwnedToken(HANDLE);

    impl Drop for OwnedToken {
        fn drop(&mut self) {
            // SAFETY: `OpenProcessToken`が開いたハンドルで、1度だけ閉じる
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Windowsが返した、0で終わるUTF-16の文字列を読む
    ///
    /// # Safety
    ///
    /// `string`は、0で終わる文字列を指すこと
    unsafe fn read_wide(string: *const u16) -> String {
        // SAFETY: 呼び出し側が、0で終わることを保証する
        let len = (0..)
            .take_while(|&i| unsafe { *string.add(i) } != 0)
            .count();
        // SAFETY: 上で数えた長さだけ読む
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(string, len) })
    }

    /// 今のプロセスのユーザーのSID（`S-1-5-21-…`）
    pub(super) fn current_user_sid() -> io::Result<String> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: 今のプロセスの疑似ハンドルと、書き込み先のポインターを渡す
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedToken(token);

        let mut needed = 0u32;
        // SAFETY: 大きさを聞くだけ（バッファは渡さない）。失敗するのが正しい
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        // TOKEN_USERの並びに合わせるため、u64の配列で確保する
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: `needed`バイト以上のバッファを渡す
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: 成功したので、バッファの先頭はTOKEN_USER
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };

        let mut string: *mut u16 = std::ptr::null_mut();
        // SAFETY: `sid`は`buffer`の中にあり、`buffer`はまだ生きている
        if unsafe { ConvertSidToStringSidW(sid, &mut string) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let string = LocalBox(string);
        // SAFETY: Windowsが返した、0で終わる文字列
        Ok(unsafe { read_wide(string.0) })
    }

    /// 本人・SYSTEM・Administratorsだけが使え、親から受け継がないACL
    /// （`P`は受け継がない印）
    pub(super) fn private_sddl(user_sid: &str) -> String {
        format!("D:P(A;;FA;;;{user_sid})(A;;FA;;;SY)(A;;FA;;;BA)")
    }

    fn security_descriptor(sddl: &str) -> io::Result<LocalBox<std::ffi::c_void>> {
        let sddl = wide(sddl.as_ref());
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: 0で終わるSDDLと、書き込み先のポインターを渡す
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(LocalBox(descriptor))
    }

    pub(super) fn create_new_private(path: &Path) -> io::Result<File> {
        let descriptor = security_descriptor(&private_sddl(&current_user_sid()?))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let path = long_path(path)?;
        let name = wide(path.as_os_str());
        // std::fs::OpenOptionsの`create_new`と同じく、既にあれば失敗する。
        // 共有の仕方もstdと同じ
        // SAFETY: 0で終わるパスと、生きているSECURITY_ATTRIBUTESを渡す
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: 開いたばかりのハンドルで、ほかに持ち主はいない
        let file = unsafe { File::from_raw_handle(handle) };
        // ACLを確認できるまでは、呼び出し元に渡さない。失敗時は空のファイルを消す。
        let acl_result = match supports_acls(&file) {
            Ok(true) => Ok(()),
            Ok(false) => Err(io::Error::other(format!(
                "cannot make {} readable by you only: the file system has no permissions (ACLs), such as FAT32 or exFAT. Use a folder on an NTFS drive",
                path.display()
            ))),
            // SMB 3.0等でボリューム情報を取得できないときは、作成時に渡した
            // DACL（継承の保護も含む）が実際に付いたことをハンドルから確認する。
            Err(_) => verify_private_dacl(&file, &descriptor),
        };
        if let Err(error) = acl_result {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(file)
    }

    /// ファイルのあるボリュームが、ACLを保てるか（`FILE_PERSISTENT_ACLS`）
    fn supports_acls(file: &File) -> io::Result<bool> {
        // Win32::System::SystemServicesの値（そのためだけに機能を足さない）
        const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
        let mut flags = 0u32;
        // SAFETY: 開いているファイルのハンドルと、書き込み先のポインターを
        // 渡す。名前は要らないので、バッファは渡さない
        if unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                std::ptr::null_mut(),
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & FILE_PERSISTENT_ACLS != 0)
    }

    fn verify_private_dacl(file: &File, expected: &LocalBox<std::ffi::c_void>) -> io::Result<()> {
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut actual: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: GENERIC_READで開いたハンドル（READ_CONTROLを含む）と、
        // セキュリティ記述子の書き込み先を渡す。
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut actual,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let actual = LocalBox(actual);
        let mut control = 0u16;
        let mut revision = 0u32;
        // SAFETY: 上で得たセキュリティ記述子を渡す。
        if unsafe { GetSecurityDescriptorControl(actual.0, &mut control, &mut revision) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // NULL DACLは誰でも使える。保護されていても、ACEの内容まで確かめる。
        if dacl.is_null()
            || control & SE_DACL_PROTECTED == 0
            || descriptor_to_sddl(&actual)? != descriptor_to_sddl(expected)?
        {
            return Err(io::Error::other("the file did not retain its private ACL"));
        }
        Ok(())
    }

    fn descriptor_to_sddl(descriptor: &LocalBox<std::ffi::c_void>) -> io::Result<String> {
        let mut string: *mut u16 = std::ptr::null_mut();
        // SAFETY: 生きているセキュリティ記述子と書き込み先を渡す。
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.0,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut string,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let string = LocalBox(string);
        // SAFETY: Windowsが返した、0で終わる文字列。
        Ok(unsafe { read_wide(string.0) })
    }

    /// `CreateFileW`に渡すパス。std::fsと同じく、絶対パスにして`MAX_PATH`
    /// 以上になるときは、`\\?\`を付ける（PR #678のレビュー）。短いパスは、
    /// そのまま渡す
    fn long_path(path: &Path) -> io::Result<std::path::PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        const MAX_PATH: usize = 260;
        let backslash = u16::from(b'\\');
        let units: Vec<u16> = path.as_os_str().encode_wide().collect();
        // 既に`\\?\`・`\\.\`の付いたパスは、そのまま
        let device = |c: u8| {
            units.len() >= 4
                && units[0] == backslash
                && units[1] == backslash
                && units[2] == u16::from(c)
                && units[3] == backslash
        };
        if device(b'?') || device(b'.') {
            return Ok(path.to_path_buf());
        }
        // 短い相対パスでも、今のフォルダが深ければ長くなるので、絶対パスの
        // 長さで決める。`absolute`は、`.`・`..`も解決する（GetFullPathNameW）
        let absolute = std::path::absolute(path)?;
        let absolute: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        if absolute.len() < MAX_PATH {
            return Ok(path.to_path_buf());
        }
        // 単独のサロゲートも保つため、UTF-16の列のまま接頭辞を付ける
        let (prefix, body): (&str, &[u16]) = match absolute.strip_prefix(&[backslash, backslash]) {
            Some(unc) => (r"\\?\UNC\", unc),
            None => (r"\\?\", &absolute[..]),
        };
        let verbatim: Vec<u16> = prefix.encode_utf16().chain(body.iter().copied()).collect();
        Ok(std::path::PathBuf::from(std::ffi::OsString::from_wide(
            &verbatim,
        )))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn starts_with(path: &Path, prefix: &str) -> bool {
            let units: Vec<u16> = path.as_os_str().encode_wide().collect();
            let prefix: Vec<u16> = prefix.encode_utf16().collect();
            units.starts_with(&prefix)
        }

        #[test]
        fn acl_fallback_checks_the_dacl_on_the_created_file() {
            let dir = tempfile::tempdir().unwrap();
            let file = create_new_private(&dir.path().join("secret.toml")).unwrap();
            let expected =
                security_descriptor(&private_sddl(&current_user_sid().unwrap())).unwrap();
            verify_private_dacl(&file, &expected).unwrap();
            let different = security_descriptor("D:P(A;;FA;;;WD)").unwrap();
            assert!(verify_private_dacl(&file, &different).is_err());

            let inherited = dir.path().join("inherited.toml");
            drop(File::create(&inherited).unwrap());
            let file = File::open(&inherited).unwrap();
            assert!(verify_private_dacl(&file, &expected).is_err());
        }

        #[test]
        fn short_relative_path_can_point_to_a_long_absolute_path() {
            const CHILD: &str = "MCP_RS_PRIVATE_FILE_LONG_PATH_TEST_CHILD";
            let relative = format!("{}.toml", "a".repeat(60));
            let relative = Path::new(&relative);

            if std::env::var_os(CHILD).is_some() {
                let short = Path::new("secret.toml");
                assert_eq!(long_path(short).unwrap(), short);
                assert!(relative.as_os_str().encode_wide().count() < 260);
                assert!(
                    std::path::absolute(relative)
                        .unwrap()
                        .as_os_str()
                        .encode_wide()
                        .count()
                        >= 260
                );
                let path = long_path(relative).unwrap();
                assert!(starts_with(&path, r"\\?\"));
                assert_eq!(long_path(&path).unwrap(), path);
                drop(create_new_private(relative).unwrap());
                return;
            }

            // 子プロセスだけの作業ディレクトリを変え、並行するテストに影響させない。
            let dir = tempfile::tempdir().unwrap();
            let mut cwd = dir.path().to_path_buf();
            while cwd.as_os_str().encode_wide().count() < 200 {
                cwd.push("a".repeat(20));
            }
            std::fs::create_dir_all(&cwd).unwrap();
            let test_name = module_path!().split_once("::").unwrap().1;
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(format!(
                    "{test_name}::short_relative_path_can_point_to_a_long_absolute_path"
                ))
                .current_dir(&cwd)
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            test_support::assert_private(&cwd.join(relative));
        }

        #[test]
        fn long_path_keeps_unc_and_device_paths() {
            let unc = format!(r"\\server\share\{}\secret.toml", "a".repeat(250));
            let converted = long_path(Path::new(&unc)).unwrap();
            assert!(
                starts_with(&converted, r"\\?\UNC\server\share\"),
                "{converted:?}"
            );

            let verbatim = format!(r"\\?\C:\{}\secret.toml", "a".repeat(250));
            assert_eq!(
                long_path(Path::new(&verbatim)).unwrap(),
                Path::new(&verbatim)
            );
        }

        /// 単独のサロゲートを、U+FFFDに変えずに保つ（PR #678のレビュー）
        #[test]
        fn long_path_keeps_lone_surrogates() {
            use std::os::windows::ffi::OsStringExt;
            let mut units: Vec<u16> = format!(r"C:\{}\", "a".repeat(260)).encode_utf16().collect();
            units.push(0xD800);
            let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&units));
            let converted = long_path(&path).unwrap();
            let converted: Vec<u16> = converted.as_os_str().encode_wide().collect();
            assert_eq!(converted.last(), Some(&0xD800));
        }
    }

    #[cfg(test)]
    pub(crate) mod test_support {
        use std::path::Path;

        use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
        use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

        use super::{descriptor_to_sddl, security_descriptor, wide, LocalBox};

        /// ファイルのACL（DACL）を、SDDLの文字列で返す
        pub(crate) fn dacl_sddl(path: &Path) -> String {
            let name = wide(super::long_path(path).unwrap().as_os_str());
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            // SAFETY: 0で終わるパスと、書き込み先のポインターを渡す
            let status = unsafe {
                GetNamedSecurityInfoW(
                    name.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut descriptor,
                )
            };
            assert_eq!(
                status,
                0,
                "GetNamedSecurityInfoW failed for {}",
                path.display()
            );
            let descriptor = LocalBox(descriptor);
            descriptor_to_sddl(&descriptor).unwrap()
        }

        /// 本人・SYSTEM・Administratorsだけで、受け継いでいないACLか
        pub(crate) fn assert_private(path: &Path) {
            let sddl = dacl_sddl(path);
            // Windowsは、よく知られたSIDを短い名前で書く（組み込みの
            // Administratorは`LA`等）。期待する値も、同じ変換を通して比べる
            let descriptor =
                security_descriptor(&super::private_sddl(&super::current_user_sid().unwrap()))
                    .unwrap();
            let expected = descriptor_to_sddl(&descriptor).unwrap();
            assert_eq!(sddl, expected, "ACL of {}", path.display());
        }
    }
}

#[cfg(all(test, windows))]
pub(crate) use windows::test_support::assert_private as assert_private_on_windows;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn create_new_private_writes_and_refuses_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        let mut file = create_new_private(&path).unwrap();
        file.write_all(b"password = \"x\"").unwrap();
        drop(file);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "password = \"x\"");
        let error = create_new_private(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[cfg(unix)]
    #[test]
    fn create_new_private_is_600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        drop(create_new_private(&path).unwrap());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(windows)]
    #[test]
    fn create_new_private_limits_the_acl_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.toml");
        drop(create_new_private(&path).unwrap());
        assert_private_on_windows(&path);
    }

    /// 長いパス（MAX_PATH以上）にも作れる（PR #678のレビュー）
    #[cfg(windows)]
    #[test]
    fn create_new_private_handles_a_long_path_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let mut path = dir.path().to_path_buf();
        while path.as_os_str().len() < 300 {
            path.push("a".repeat(40));
        }
        std::fs::create_dir_all(&path).unwrap();
        let path = path.join("secret.toml");
        let mut file = create_new_private(&path).unwrap();
        file.write_all(b"x").unwrap();
        drop(file);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
        assert_private_on_windows(&path);
    }

    #[cfg(windows)]
    #[test]
    fn rename_keeps_the_private_acl_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        let temporary = dir.path().join(".secret.toml.tmp");
        let path = dir.path().join("secret.toml");
        drop(create_new_private(&temporary).unwrap());
        std::fs::rename(&temporary, &path).unwrap();
        assert_private_on_windows(&path);
    }
}
