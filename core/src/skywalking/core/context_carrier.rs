// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/// The Injectable implementation supports inject the give key/value for further propagation,
/// Such as putting a key/value into the HTTP header.
pub trait Injectable {
    /// Inject the given key/value into the implementation.
    /// The way of injection is determined by the implementation, no panic! should happens even injection fails.
    fn inject(&self, key: String, value: String);
}

/// The Extractable implementations extract propagated context out the implementation.
/// Such as fetching the key/value from the HTTP header.
pub trait Extractable {
    /// Fetch the value by the given key.
    fn extract(&self, key: String) -> &str;
}