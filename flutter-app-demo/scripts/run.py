#!/usr/bin/env python3
import os, sys
import re
import subprocess
import signal
import sys
import threading
from lib import look_for_proj_dir

def signal_handler(signal, frame):
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)

FLUTTER = 'flutter.bat' if sys.platform == 'win32' else 'flutter'
PROJ_DIR = look_for_proj_dir(os.path.abspath(__file__), 'pubspec.yaml')
RUST_PROJ_DIR = os.path.join(PROJ_DIR, 'rust')

class CargoThread(threading.Thread):
    def __init__(self):
        super().__init__()
        self.observatory_uri = ''
        self.observatory_open = threading.Event()

    def run(self):
        self.proc = subprocess.Popen(['cargo', 'run'],
            stdout = subprocess.PIPE,
            cwd = RUST_PROJ_DIR
        )

        while True:
            line = self.proc.stdout.readline()
            if self.proc.poll() is not None:
                # proc ended
                return
            print(line.decode(), end = '')
            match = re.search(r'Observatory listening on (?P<schema>https?://)(\S*)', line.decode())

            if match:
                self.observatory_uri = match.group('schema') + match.group(2)
                self.observatory_open.set()


def cargo_run():
    cargo = CargoThread()
    cargo.start()
    cargo.observatory_open.wait()
    return cargo.observatory_uri

if __name__ == '__main__':
    print('🍀  Building flutter bundle')
    subprocess.run(
        [FLUTTER, 'build', 'bundle'],
        cwd = PROJ_DIR, check = True)

    print('🦀  Building rust project')
    uri = cargo_run()
    if not uri:
        raise Exception('Launch cargo error')

    print('🍹  Attaching dart debugger')
    subprocess.run(
        [FLUTTER, 'attach', '--device-id=flutter-tester', '--debug-uri=' + uri],
        cwd = PROJ_DIR, check = True)
