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

use skywalking::{
    management::{instance::Properties, manager::Manager},
    proto::v3::{InstancePingPkg, InstanceProperties, KeyStringValuePair},
    reporter::{CollectItem, Report},
};
use std::{
    collections::LinkedList,
    process,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn management() {
    let reporter = Arc::new(MockReporter::default());
    let manager = Manager::new("service_name", "instance_name", reporter.clone());
    let handling = manager.report_and_keep_alive(
        || {
            let mut props = Properties::new();
            props.insert_os_info();
            props
        },
        Duration::from_millis(100),
        3,
    );

    sleep(Duration::from_secs(1)).await;

    handling.handle().abort();

    sleep(Duration::from_secs(1)).await;

    {
        let actual_props = reporter.pop_ins_props();
        assert_eq!(actual_props.service, "service_name".to_owned());
        assert_eq!(actual_props.service_instance, "instance_name".to_owned());
        assert_eq!(
            kvs_get_value(&actual_props.properties, Properties::KEY_LANGUAGE),
            "rust"
        );
        assert_eq!(
            kvs_get_value(&actual_props.properties, Properties::KEY_HOST_NAME),
            hostname::get().unwrap()
        );
        assert_eq!(
            kvs_get_value(&actual_props.properties, Properties::KEY_PROCESS_NO),
            process::id().to_string()
        );
    }

    {
        assert_eq!(
            reporter.pop_ping(),
            InstancePingPkg {
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                ..Default::default()
            }
        );
    }

    {
        reporter.pop_ping();
    }

    {
        reporter.pop_ins_props();
    }

    {
        reporter.pop_ping();
    }

    {
        reporter.pop_ping();
    }
}

fn kvs_get_value<'a>(kvs: &'a [KeyStringValuePair], key: &str) -> &'a str {
    &kvs.iter().find(|kv| kv.key == key).unwrap().value
}

#[derive(Debug)]
enum Item {
    Properties(InstanceProperties),
    PingPkg(InstancePingPkg),
}

impl Item {
    fn unwrap_properties(self) -> InstanceProperties {
        match self {
            Item::Properties(props) => props,
            Item::PingPkg(_) => panic!("isn't properties"),
        }
    }

    fn unwrap_ping_pkg(self) -> InstancePingPkg {
        match self {
            Item::Properties(_) => panic!("isn't ping pkg"),
            Item::PingPkg(p) => p,
        }
    }
}

#[derive(Default, Clone)]
struct MockReporter {
    items: Arc<Mutex<LinkedList<Item>>>,
}

impl MockReporter {
    fn pop_ins_props(&self) -> InstanceProperties {
        self.items
            .try_lock()
            .unwrap()
            .pop_front()
            .unwrap()
            .unwrap_properties()
    }

    fn pop_ping(&self) -> InstancePingPkg {
        self.items
            .try_lock()
            .unwrap()
            .pop_front()
            .unwrap()
            .unwrap_ping_pkg()
    }
}

impl Report for MockReporter {
    fn report(&self, item: CollectItem) {
        match item {
            CollectItem::Instance(data) => {
                self.items
                    .try_lock()
                    .unwrap()
                    .push_back(Item::Properties(*data));
            }
            CollectItem::Ping(data) => {
                self.items
                    .try_lock()
                    .unwrap()
                    .push_back(Item::PingPkg(*data));
            }
            _ => {
                unreachable!("unknown collect item type");
            }
        }
    }
}
