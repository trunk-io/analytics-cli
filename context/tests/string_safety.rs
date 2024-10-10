use context::string_safety::safe_truncate_str;

#[test]
fn test_safe_truncate_str() {
    pretty_assertions::assert_eq!("trunk", safe_truncate_str::<5>("trunkate me!"));
    pretty_assertions::assert_eq!(
        "trunkate me!",
        safe_truncate_str::<100>("      trunkate me!      ")
    );
    pretty_assertions::assert_eq!("trunk", safe_truncate_str::<5>("      trunkate me!      "));
}
