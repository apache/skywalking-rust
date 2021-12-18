import os
import sys
import time
from difflib import Differ
from os.path import dirname

import argparse
import yaml
import requests
import time

try:
  from yaml import CSafeLoader as Loader
except ImportError:
  from yaml import SafeLoader as Loader

def validate(expected_file_name):
  with open(expected_file_name) as expected_data_file:
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

if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  parser.add_argument('--expected_file', help='File name which includes expected reported value')
  parser.add_argument('--max_retry_times', help='Max retry times', type=int)
  parser.add_argument('--target_path', help='Specify target path')
  
  args = parser.parse_args()

  retry_times = 0
  while True:
    if retry_times > args.max_retry_times:
      raise RuntimeError("Max retry times exceeded")

    try:
      requests.get('http://0.0.0.0:8081{0}'.format(args.target_path), timeout=5)
    except Exception as e:
      print(e)
      retry_times += 1
      time.sleep(2)
      continue

    res = validate(args.expected_file)
    assert res.status_code == 200
    break
