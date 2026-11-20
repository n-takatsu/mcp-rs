//! 本人だけが読み書きできるファイルを、作るときから制限して作る（#599・#674）。
//!
//! Unixでは、モード`600`で作る。Windowsでは、本人とSYSTEM・Administrators
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
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
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
    use std::os::windows::io::FromRawHandle;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE,
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

    pub(super) fn create_new_private(path: &Path) -> io::Result<File> {
        let sddl = wide(private_sddl(&current_user_sid()?).as_ref());
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
        let descriptor = LocalBox(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
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
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    #[cfg(test)]
    pub(crate) mod test_support {
        use std::path::Path;

        use windows_sys::Win32::Security::Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        };
        use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

        use super::{read_wide, wide, LocalBox};

        /// ファイルのACL（DACL）を、SDDLの文字列で返す
        pub(crate) fn dacl_sddl(path: &Path) -> String {
            let name = wide(path.as_os_str());
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
            descriptor_to_sddl(descriptor.0)
        }

        /// セキュリティ記述子のDACLを、SDDLの文字列にする
        fn descriptor_to_sddl(descriptor: PSECURITY_DESCRIPTOR) -> String {
            let mut string: *mut u16 = std::ptr::null_mut();
            // SAFETY: 呼び出し側が得た、生きているセキュリティ記述子を渡す
            let ok = unsafe {
                ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    descriptor,
                    SDDL_REVISION_1,
                    DACL_SECURITY_INFORMATION,
                    &mut string,
                    std::ptr::null_mut(),
                )
            };
            assert_ne!(
                ok, 0,
                "ConvertSecurityDescriptorToStringSecurityDescriptorW failed"
            );
            let string = LocalBox(string);
            // SAFETY: Windowsが返した、0で終わる文字列
            unsafe { read_wide(string.0) }
        }

        /// 本人・SYSTEM・Administratorsだけで、受け継いでいないACLか
        pub(crate) fn assert_private(path: &Path) {
            let sddl = dacl_sddl(path);
            // Windowsは、よく知られたSIDを短い名前で書く（組み込みの
            // Administratorは`LA`等）。期待する値も、同じ変換を通して比べる
            let expected = wide(super::private_sddl(&super::current_user_sid().unwrap()).as_ref());
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            // SAFETY: 0で終わるSDDLと、書き込み先のポインターを渡す
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    expected.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };
            assert_ne!(
                ok, 0,
                "ConvertStringSecurityDescriptorToSecurityDescriptorW failed"
            );
            let descriptor = LocalBox(descriptor);
            let expected = descriptor_to_sddl(descriptor.0);
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
