fn main() {
    println!("Hello, world!");
}

/// A simple function that adds two numbers.
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_sum() {
        let result = calculate_sum(2, 3);
        println!("Result: {}", result);
    }

    #[test]
    fn test_calculate_multiply() {
        let result = calculate_multiply(2, 3);
        println!("Result: {}", result);
    }
}

pub fn calculate_multiply(a: i32, b: i32) -> i32 {
    return a * b;
}
