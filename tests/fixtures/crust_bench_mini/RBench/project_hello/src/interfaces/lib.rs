pub fn hello() -> String {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello(), "hello");
    }
}
