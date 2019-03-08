import os
import subprocess
from shutil import copyfile, copytree, rmtree

def prepare(envs):
    return dict(envs)

def build(envs):
    print(envs)

snap_tmpl = '''name: flutter-app
version: '0.1'
summary: Yet another amazing flutter app
description: |
  This is my-snap's description. You have a paragraph or two to tell the
  most important story about your snap. Keep it under 100 words though,
  we live in tweetspace and your description wants to look good in the snap
  store.

grade: devel
confinement: flutter-app

apps:
  flutter-app:
    command: bin/flutter-app

parts:
  flutter-app:
    source: .
    plugin: rust
'''
