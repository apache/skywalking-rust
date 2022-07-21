use crate::context::system_time::{fetch_time, TimePeriod};

tonic::include_proto!("skywalking.v3");

impl SpanObject {
    /// Add logs to the span.
    pub fn add_log<K, V, I>(&mut self, message: I)
    where
        K: ToString,
        V: ToString,
        I: IntoIterator<Item = (K, V)>,
    {
        let log = Log {
            time: fetch_time(TimePeriod::Log),
            data: message
                .into_iter()
                .map(|v| {
                    let (key, value) = v;
                    KeyStringValuePair {
                        key: key.to_string(),
                        value: value.to_string(),
                    }
                })
                .collect(),
        };
        self.logs.push(log);
    }

    /// Add tag to the span.
    pub fn add_tag(&mut self, key: impl ToString, value: impl ToString) {
        self.tags.push(KeyStringValuePair {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
}
