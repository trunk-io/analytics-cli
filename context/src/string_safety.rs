pub fn safe_truncate_string<const MAX_LEN: usize, T: AsRef<str>>(value: &T) -> &str {
    safe_truncate_str::<MAX_LEN>(value.as_ref())
}

pub fn safe_truncate_str<const MAX_LEN: usize>(value: &str) -> &str {
    &value.trim()[..value.trim().floor_char_boundary(MAX_LEN)]
}

#[derive(Debug, Clone)]
pub enum FieldLen {
    TooShort(String),
    TooLong(String),
    Valid,
}

pub fn validate_field_len<const MAX_LEN: usize, T: AsRef<str>>(field: T) -> FieldLen {
    let trimmed_field = field.as_ref().trim();
    let trimmed_field_len = trimmed_field.len();

    if trimmed_field_len == 0 {
        FieldLen::TooShort(String::from(trimmed_field))
    } else if (1..=MAX_LEN).contains(&trimmed_field_len) {
        FieldLen::Valid
    } else {
        FieldLen::TooLong(String::from(safe_truncate_string::<MAX_LEN, _>(
            &trimmed_field,
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::string_safety::safe_truncate_str;

    #[test]
    fn test_safe_truncate_str() {
        pretty_assertions::assert_eq!("trunk", safe_truncate_str::<5>("trunkate me!"));
        pretty_assertions::assert_eq!("trunkate me!", safe_truncate_str::<100>(" trunkate me! "));
        pretty_assertions::assert_eq!("trunk", safe_truncate_str::<5>(" trunkate me! "));
    }
}
