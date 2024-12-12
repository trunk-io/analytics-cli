pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {

    #[test]
    fn always_fails() {
        assert_eq!(1, 2)
    }

    #[test]
    fn always_succeeds() {
        assert_eq!(1, 1)
    }
}
