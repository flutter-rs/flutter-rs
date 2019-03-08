import os
import subprocess
import tempfile

def prepare(envs):
    return dict(envs)

def build(envs):
    f = tempfile.NamedTemporaryFile(mode='w+', delete=False)
    s = tmpl.format(
        **envs,
    )
    f.write(s)
    f.close()

    output = os.path.join(envs['OUTPUT_DIR'], envs['NAME'] + '.dmg')

    subprocess.run([
        'dmgbuild', '-s', f.name,
        envs['NAME'],
        output,
    ], check = True)

    os.unlink(f.name)

    print('>>> dmg generated at', output)


tmpl = '''
import os

files = ['{APP_PATH}']
symlinks = {{ 'Applications': '/Applications' }}

# badge_icon = os.path.join('{RUST_ASSETS_DIR}', 'icon.icns')
icon = os.path.join('{RUST_ASSETS_DIR}', 'icon.icns')

# Where to put the icons
icon_locations = {{
    '{APP_NAME}':  (140, 120),
    'Applications': (500, 120)
}}

background = 'builtin-arrow'

show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

show_icon_preview = False

default_view = 'icon-view'
'''
