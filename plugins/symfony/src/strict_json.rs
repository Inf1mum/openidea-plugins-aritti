//! Reused repository-owned JSON admission helper; no external plugin source.
//! Standard JSON with decoded-key uniqueness and bounded nesting/value count.
use openidea_plugin::Error;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
struct Seed<'a> {
    depth: usize,
    remaining: &'a mut usize,
}
struct ValueVisitor<'a>(Seed<'a>);
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        if self.depth > 64 || *self.remaining == 0 {
            return Err(de::Error::custom("JSON depth or value budget exceeded"));
        }
        *self.remaining -= 1;
        decoder.deserialize_any(ValueVisitor(self))
    }
}
impl<'de> Visitor<'de> for ValueVisitor<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded JSON without duplicate object keys")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            depth: self.0.depth + 1,
            remaining: &mut *self.0.remaining,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            let value = map.next_value_seed(Seed {
                depth: self.0.depth + 1,
                remaining: &mut *self.0.remaining,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
pub(crate) fn parse(text: &str, limit: usize) -> Result<Value, Error> {
    if text.len() > limit {
        return Err(Error::Stale);
    }
    let mut decoder = serde_json::Deserializer::from_str(text);
    let mut remaining = 50_000;
    let value = Seed {
        depth: 0,
        remaining: &mut remaining,
    }
    .deserialize(&mut decoder)
    .map_err(|_| Error::Stale)?;
    decoder.end().map_err(|_| Error::Stale)?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_key_uniqueness_handles_escaped_unicode_without_flattened_path_collisions() {
        for raw in [
            r#"{"a":1,"a":2}"#,
            r#"{"\u0061":1,"a":2}"#,
            r#"{"x":{"a":1,"a":2}}"#,
            r#"{"\ud83d\ude00":1,"😀":2}"#,
        ] {
            assert!(parse(raw, 128 << 10).is_err(), "{raw}");
        }
        let value = parse(r#"{"a.b":1,"a":{"b":2},"x":"\ud83d\ude00"}"#, 128 << 10).unwrap();
        assert_eq!(value["x"], "😀");
        assert_eq!(value["a.b"], 1);
        assert_eq!(value["a"]["b"], 2);
    }
    #[test]
    fn excessive_depth_values_bytes_and_trailing_data_are_refused() {
        assert!(parse(&format!("{}0{}", "[".repeat(65), "]".repeat(65)), 128 << 10).is_err());
        assert!(parse(&format!("[{}0]", "0,".repeat(50_000)), 128 << 10).is_err());
        for text in ["{}{}", "{x:1}", "[NaN]", r#""\ud83d""#] {
            assert!(parse(text, 128 << 10).is_err());
        }
        assert!(parse("null", 3).is_err());
    }
}
