pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {

    #[test]
    fn always_succeeds() {
        assert_eq!(1, 1)
    }
}
