pub fn safe_truncate_string<'a, const MAX_LEN: usize, T: AsRef<str>>(value: &'a T) -> &'a str {
    safe_truncate_str::<MAX_LEN>(value.as_ref())
}

pub fn safe_truncate_str<'a, const MAX_LEN: usize>(value: &'a str) -> &'a str {
    &value.trim()[..value.floor_char_boundary(MAX_LEN)]
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
