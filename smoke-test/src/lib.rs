pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {

    #[test]
    fn always_succeeds() {
        assert_eq!(1, 1)
    }

    #[test]
    fn fails_if_fail_env_is_present() {
        // Using an env var so that we only fail when running smoke tests
        if std::env::var("FAIL_TEST").is_ok() {
            assert_eq!(1, 2)
        } else {
            assert_eq!(1, 1)
        }
    }
}
