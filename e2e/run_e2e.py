# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

import sys
from difflib import Differ
from pathlib import Path
from retry.api import retry_call

import argparse
import yaml
import requests

try:
  from yaml import CSafeLoader as Loader
except ImportError:
  from yaml import SafeLoader as Loader

def validate(expected_data):
  response = requests.post(url='http://0.0.0.0:12800/dataValidate', data=expected_data)
  if response.status_code != 200:
    raise Exception('data validate failed')

def health_check():
  requests.get('http://0.0.0.0:8081/healthCheck', timeout=5)

def call_target_path(target_path):
  requests.get('http://0.0.0.0:8081{0}'.format(target_path), timeout=5)


if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  parser.add_argument('--expected_file', help='File name which includes expected reported value')
  parser.add_argument('--max_retry_times', help='Max retry times', type=int)
  parser.add_argument('--target_path', help='Specify target path')
  
  args = parser.parse_args()

  import logging
  logging.basicConfig()
  retry_call(health_check, tries=30, delay=2)
  retry_call(call_target_path, fargs=[args.target_path], tries=args.max_retry_times, delay=2)

  expected_data = Path(args.expected_file).read_text()

  try:
    retry_call(validate, fargs=[expected_data], tries=args.max_retry_times, delay=2)
  except Exception as e:
    res = requests.get('http://0.0.0.0:12800/receiveData')
    actual_data = yaml.dump(yaml.load(res.content, Loader=Loader))

    differ = Differ()
    diff_list = list(differ.compare(
      actual_data.splitlines(keepends=True),
      yaml.dump(yaml.load(expected_data, Loader=Loader)).splitlines(keepends=True)
    ))

    print('diff list: ')
    sys.stdout.writelines(diff_list)
    raise e
