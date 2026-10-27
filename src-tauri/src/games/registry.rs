//! Lectura (només lectura) del registre de Windows.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{HSTRING, PWSTR};

pub use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// Valor de text (`REG_SZ`) o `None` si la clau o el valor no existeixen.
pub fn read_string(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let (subkey, value) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut size = 0u32;
    // Primera crida: mida en bytes; segona: el text
    let r = unsafe { RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, None, Some(&mut size)) };
    if r != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let r = unsafe {
        RegGetValueW(root, &subkey, &value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size))
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Noms de les subclaus directes.
pub fn subkeys(root: HKEY, path: &str) -> Vec<String> {
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        let r = unsafe { RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) };
        if r != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_well_known_windows_keys() {
        let product = read_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "ProductName");
        assert!(product.is_some_and(|p| p.contains("Windows")));
        assert!(read_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\NoExisteix\Res", "x").is_none());
        assert!(!subkeys(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft").is_empty());
        assert!(subkeys(HKEY_LOCAL_MACHINE, r"SOFTWARE\NoExisteix").is_empty());
    }
}
