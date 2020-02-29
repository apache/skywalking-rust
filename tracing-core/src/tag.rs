#[derive(Clone)]
pub struct Tag {
    key: String,
    value: String,
}

impl Tag {
    pub fn New(key: String, value: String) -> Self {
        Tag {
            key,
            value,
        }
    }

    pub fn key(&self) -> String {
        self.key.clone()
    }

    pub fn value(&self) -> String {
        self.value.clone()
    }
}

#[cfg(test)]
mod tag_tests {
    use crate::Tag;

    #[test]
    fn test_tag_new() {
        let tag = Tag::New(String::from("tag_key"), String::from("tag_value"));
        assert_eq!(tag.key, "tag_key");
        assert_eq!(tag.value, "tag_value");
        let tag_clone = tag.clone();
        assert_eq!(tag_clone.key, "tag_key");
        assert_eq!(tag_clone.value, "tag_value");
    }
}