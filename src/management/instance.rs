// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Skywalking report instance properties items.

use crate::proto::v3::{InstanceProperties, KeyStringValuePair};
use once_cell::sync::Lazy;
use std::{collections::HashMap, process};
use systemstat::{IpAddr, Platform, System};

static IPS: Lazy<Vec<String>> = Lazy::new(|| {
    System::new()
        .networks()
        .ok()
        .map(|networks| {
            networks
                .values()
                .filter(|network| {
                    network.name != "lo"
                        && !network.name.starts_with("docker")
                        && !network.name.starts_with("br-")
                })
                .flat_map(|network| {
                    network.addrs.iter().filter_map(|addr| match addr.addr {
                        IpAddr::V4(addr) => Some(addr.to_string()),
                        _ => None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
});

static HOST_NAME: Lazy<Option<String>> = Lazy::new(|| {
    hostname::get()
        .ok()
        .and_then(|hostname| hostname.into_string().ok())
});

const OS_NAME: Option<&str> = if cfg!(target_os = "linux") {
    Some("Linux")
} else if cfg!(target_os = "windows") {
    Some("Windows")
} else if cfg!(target_os = "macos") {
    Some("Mac OS X")
} else {
    None
};

/// Builder of [InstanceProperties].
#[derive(Debug, Default)]
pub struct Properties {
    inner: HashMap<String, Vec<String>>,
}

impl Properties {
    /// Instance properties key of host name.
    pub const KEY_HOST_NAME: &'static str = "hostname";
    /// Instance properties key of ipv4.
    pub const KEY_IPV4: &'static str = "ipv4";
    /// Instance properties key of programming language.
    pub const KEY_LANGUAGE: &'static str = "language";
    /// Instance properties key of os name.
    pub const KEY_OS_NAME: &'static str = "OS Name";
    /// Instance properties key of process number.
    pub const KEY_PROCESS_NO: &'static str = "Process No.";
}

impl Properties {
    /// New empty properties.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Insert key value pair, will not overwrite the original, because multiple
    /// values of the same key can exist.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.entry(key.into()).or_default().push(value.into());
    }

    /// Overwrite the values, whether there are multiple.
    pub fn update(&mut self, key: &str, value: impl Into<String>) {
        if let Some(values) = self.inner.get_mut(key) {
            *values = vec![value.into()];
        }
    }

    /// Remove all values associated the key.
    pub fn remove(&mut self, key: &str) {
        self.inner.remove(key);
    }

    /// Insert the OS system info, such as os name, host name, etc.
    pub fn insert_os_info(&mut self) {
        for (key, value) in build_os_info() {
            self.insert(key, value);
        }
    }

    pub(crate) fn convert_to_instance_properties(
        self,
        service_name: String,
        instance_name: String,
    ) -> InstanceProperties {
        let mut properties = Vec::new();
        for (key, values) in self.inner {
            for value in values {
                properties.push(KeyStringValuePair {
                    key: key.clone(),
                    value,
                });
            }
        }

        InstanceProperties {
            service: service_name,
            service_instance: instance_name,
            properties,
            layer: Default::default(),
        }
    }
}

fn build_os_info() -> Vec<(String, String)> {
    let mut items = Vec::new();

    if let Some(os_name) = OS_NAME.as_ref() {
        items.push((Properties::KEY_OS_NAME.to_string(), os_name.to_string()));
    }

    if let Some(host_name) = HOST_NAME.as_ref() {
        items.push((Properties::KEY_HOST_NAME.to_string(), host_name.clone()));
    }

    for ip in IPS.iter() {
        items.push((Properties::KEY_IPV4.to_string(), ip.to_string()));
    }

    items.push((
        Properties::KEY_PROCESS_NO.to_string(),
        process::id().to_string(),
    ));

    items.push((Properties::KEY_LANGUAGE.to_string(), "rust".to_string()));

    items
}
