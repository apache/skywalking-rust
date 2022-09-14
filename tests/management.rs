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
    reporter::{CollectItem, Report},
    skywalking_proto::v3::{InstancePingPkg, InstanceProperties, KeyStringValuePair},
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
    manager.keep_alive(Duration::from_secs(60));

    {
        let mut props = Properties::new();
        props.insert_os_info();
        manager.report_properties(props);

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
        sleep(Duration::from_secs(1)).await;
        assert_eq!(
            reporter.pop_ping(),
            InstancePingPkg {
                service: "service_name".to_owned(),
                service_instance: "instance_name".to_owned(),
                ..Default::default()
            }
        );
    }
}

fn kvs_get_value<'a>(kvs: &'a [KeyStringValuePair], key: &str) -> &'a str {
    &kvs.iter().find(|kv| kv.key == key).unwrap().value
}

#[derive(Default, Clone)]
struct MockReporter {
    props_items: Arc<Mutex<LinkedList<InstanceProperties>>>,
    ping_items: Arc<Mutex<LinkedList<InstancePingPkg>>>,
}

impl MockReporter {
    fn pop_ins_props(&self) -> InstanceProperties {
        self.props_items.try_lock().unwrap().pop_back().unwrap()
    }

    fn pop_ping(&self) -> InstancePingPkg {
        self.ping_items.try_lock().unwrap().pop_back().unwrap()
    }
}

impl Report for MockReporter {
    fn report(&self, item: CollectItem) {
        match item {
            CollectItem::Instance(data) => {
                self.props_items.try_lock().unwrap().push_back(*data);
            }
            CollectItem::Ping(data) => {
                self.ping_items.try_lock().unwrap().push_back(*data);
            }
            _ => {}
        }
    }
}
