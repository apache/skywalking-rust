import os
import sys
from difflib import Differ
from retry.api import retry_call

import argparse
import yaml
import requests

try:
  from yaml import CSafeLoader as Loader
except ImportError:
  from yaml import SafeLoader as Loader

def validate(excepted_file):
  with open(excepted_file) as expected_data_file:
    expected_data = os.linesep.join(expected_data_file.readlines())

    response = requests.post(url='http://0.0.0.0:12800/dataValidate', data=expected_data)

    if response.status_code != 200:
      res = requests.get('http://0.0.0.0:12800/receiveData')
      actual_data = yaml.dump(yaml.load(res.content, Loader=Loader))

      differ = Differ()
      diff_list = list(differ.compare(
        actual_data.splitlines(keepends=True),
        yaml.dump(yaml.load(expected_data, Loader=Loader)).splitlines(keepends=True)
      ))

      print('diff list: ')
      sys.stdout.writelines(diff_list)

    return response

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

  res = validate(args.expected_file)
  assert res.status_code == 200
