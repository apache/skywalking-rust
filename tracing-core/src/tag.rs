/// Tag is a key value pair to represent an supplementary instruction for the span.
/// Common and most widely used tags could be found here,
/// https://github.com/apache/skywalking/blob/master/apm-sniffer/apm-agent-core/src/main/java/org/apache/skywalking/apm/agent/core/context/tag/Tags.java.
#[derive(Clone, Hash)]
pub struct Tag {
    key: String,
    value: String,
}

impl Tag {
    pub fn new(key: String, value: String) -> Self {
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
        let tag = Tag::new(String::from("tag_key"), String::from("tag_value"));
        assert_eq!(tag.key, "tag_key");
        assert_eq!(tag.value, "tag_value");
        let tag_clone = tag.clone();
        assert_eq!(tag_clone.key, "tag_key");
        assert_eq!(tag_clone.value, "tag_value");
    }
}