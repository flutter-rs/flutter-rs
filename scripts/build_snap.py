import os
from shutil import copyfile, copytree, rmtree
import subprocess

def build(envs):
    print('building!')
    print(envs)

snap_tmpl = '''name: listenv
version: '0.1'
summary: Single-line elevator pitch for your amazing snap
description: |
  This is my-snap's description. You have a paragraph or two to tell the
  most important story about your snap. Keep it under 100 words though,
  we live in tweetspace and your description wants to look good in the snap
  store.

grade: devel
confinement: devmode

apps:
  listenv:
    command: bin/demo-rust

parts:
  listenv:
    source: .
    plugin: rust
'''
