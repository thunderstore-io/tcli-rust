use winreg::RegKey;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to read subkey at '{0}'.")]
    RegistrySubkeyRead(String),

    #[error("Failed to read value with name '{0}' at key '{1}'.")]
    RegistryValueRead(String, String),
}

#[repr(isize)]
pub enum HKey {
    LocalMachine = -2147483641i32 as _,
}

pub struct RegKeyVal {
    pub key: String,
    pub val: String,
}

pub fn get_value_at(hkey: HKey, subkey: &str, name: &str) -> Result<String, Error> {
    open_subkey(hkey, subkey)?
        .get_value(name)
        .map_err(|_| Error::RegistryValueRead(subkey.to_string(), name.to_string()))
}

pub fn get_keys_at(hkey: HKey, subkey: &str) -> Result<Vec<String>, Error> {
    open_subkey(hkey, subkey)?
        .enum_keys()
        .collect::<Result<Vec<String>, _>>()
        .map_err(|_| Error::RegistrySubkeyRead(subkey.to_string()))
}

pub fn get_values_at(hkey: HKey, subkey: &str) -> Result<Vec<RegKeyVal>, Error> {
    open_subkey(hkey, subkey)?
        .enum_values()
        .map(|x| match x {
            Ok((key, val)) => Ok(RegKeyVal { key, val: val.to_string() }),
            Err(e) => Err(e),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::RegistrySubkeyRead(subkey.to_string()))
}

fn open_subkey(hkey: HKey, subkey: &str) -> Result<RegKey, Error> {
    let local = RegKey::predef(hkey as _);
    local.open_subkey(subkey).map_err(|_| Error::RegistrySubkeyRead(subkey.to_string()))
}
