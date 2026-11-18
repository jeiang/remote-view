fn greeting() -> &'static str {
    "Hello, remote-view!"
}

fn main() {
    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets() {
        assert_eq!(greeting(), "Hello, remote-view!");
    }
}
