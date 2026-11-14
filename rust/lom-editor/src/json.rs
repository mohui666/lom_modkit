use serde_json::Value;

// Editor copies retain object order. Avoid hashing every field of every node
// when that order matches, while preserving JSON's order-independent equality.
#[inline]
pub fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() {
                return false;
            }
            for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                if ak != bk {
                    return left == right;
                }
                if !same(av, bv) {
                    return false;
                }
            }
            true
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        _ => left == right,
    }
}
