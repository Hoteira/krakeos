use crate::sync::Mutex;
use crate::rust_alloc::string::String;

static CURRENT_USER: Mutex<Option<String>> = Mutex::new(None);

pub fn get_current_user() -> String {
    let user = CURRENT_USER.lock();
    user.clone().unwrap_or_else(|| String::from("racap"))
}

pub fn set_current_user(name: &str) {
    let mut user = CURRENT_USER.lock();
    *user = Some(String::from(name.trim()));
}
