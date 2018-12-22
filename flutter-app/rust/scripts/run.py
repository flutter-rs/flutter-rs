#!/usr/bin/env python3
import os
import re
import subprocess
from lib import look_for_proj_dir

PROJ_DIR = look_for_proj_dir(os.path.abspath(__file__))
FLUTTER_PROJ_DIR = os.path.dirname(PROJ_DIR)

def cargo_run():
    proc = subprocess.Popen(['cargo', 'run'], stdout = subprocess.PIPE, cwd = PROJ_DIR)
    while True:
        line = proc.stdout.readline()
        if not line: break
        match = re.search(r'http://(.*?):(\d+)/', line.decode())
        if match:
            return match.group(1)

if __name__ == '__main__':
    print('>>> Building flutter bundle')
    subprocess.run(['flutter', 'build', 'bundle'], cwd = FLUTTER_PROJ_DIR, check = True)

    print('>>> Building rust project')
    port = cargo_run()
    if not port:
        raise Exception('Lauch cargo error')

    print('>>> Attaching dart debugger')
    subprocess.run(['flutter', 'attach', '--device-id=flutter-tester', '--debug-port=50300'], cwd = FLUTTER_PROJ_DIR, check = True)
