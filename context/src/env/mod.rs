use std::collections::HashMap;

pub mod parser;
pub mod validator;

pub type EnvVars = HashMap<String, String>;
