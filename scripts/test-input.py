#!/usr/bin/env python3
"""Drive the native app with XTest input; use read-only capture telemetry to wait.
Run inside scripts/test-native.sh's isolated X server, never on the user's display.
"""
import json
import os
from pathlib import Path
import subprocess as sp
import sys
import time

state_dir, output = map(Path, sys.argv[1:])
root = Path.cwd()
observation = output / 'external.json'
app = None
window = None
clipboards = []
log = (output / 'external-app.log').open('w')
env = dict(os.environ, XDG_DATA_HOME=str(state_dir), STAR_BATTLE_CAPTURE_PATH=str(output / 'external.png'), STAR_BATTLE_CAPTURE_SCENARIO='external', STAR_BATTLE_TEST_UNIQUE='1')
env.pop('STAR_BATTLE_TEST_BOARD', None)

def run(*args):
    return sp.check_output(args, text=True, stderr=sp.STDOUT).strip()

def read():
    try:
        return json.loads(observation.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}

def wait(predicate, description, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        current = read()
        if predicate(current):
            return current
        if app is not None and app.poll() is not None:
            raise AssertionError(f'App exited with {app.returncode} during {description}')
        time.sleep(.1)
    raise AssertionError(f'Timed out: {description}; observation={read()}')

def click_at(x, y):
    run('xdotool', 'mousemove', '--window', window, str(round(x)), str(round(y)), 'click', '1')
    time.sleep(.16)

def click(name):
    w = read()['widgets'][name]
    assert w['visible'], f'{name} is not visible'
    click_at(w['x'], w['y'])

def key(keys):
    run('xdotool', 'key', '--clearmodifiers', keys)
    time.sleep(.16)

def cell(index):
    s = read()
    g, n = s['grid'], s['board']['size']
    return g['x'] + (index % n + .5)*g['cell'], g['y'] + (index // n + .5)*g['cell']

def mark(index, expected):
    click_at(*cell(index))
    wait(lambda s: s.get('board', {}).get('marks', [])[index] == expected, f'cell {index} becomes {expected}')

def screenshot(name):
    run('import', '-window', 'root', str(output / f'external-{name}.png'))

def clipboard(data, target):
    process = sp.Popen(['xclip', '-selection', 'clipboard', '-target', target, '-quiet'], stdin=sp.PIPE, stdout=sp.DEVNULL, stderr=sp.DEVNULL)
    clipboards.append(process)
    process.stdin.write(data)
    process.stdin.close()
    time.sleep(.15)

def start():
    global app, window
    observation.unlink(missing_ok=True)
    app = sp.Popen([str(root / 'target/debug/star-battle-solver')], env=env, stdout=log, stderr=log)
    wait(lambda s: 'widgets' in s, 'startup')
    window = run('xdotool', 'search', '--onlyvisible', '--pid', str(app.pid), '--name', 'Learn Star Battle').splitlines()[0]
    run('xdotool', 'windowfocus', '--sync', window)
    print(f'Started {app.pid}', flush=True)

def close():
    geometry = dict(line.split('=', 1) for line in run('xdotool', 'getwindowgeometry', '--shell', window).splitlines())
    click_at(int(geometry['WIDTH']) - 27, 27)
    assert app.wait(timeout=5) == 0

def saved():
    return json.loads((state_dir / 'starbattle/game.json').read_text())

def open_files(*files):
    # The secondary process must forward its file arguments and exit; the
    # observation remains owned by the original process and original window.
    secondary_env = {k: v for k, v in env.items() if not k.startswith('STAR_BATTLE_')}
    result = sp.run([str(root / 'target/debug/star-battle-solver'), *map(str, files)], env=secondary_env, stdout=log, stderr=log, timeout=5)
    assert result.returncode == 0
    assert app.poll() is None

try:
    start()
    assert read()['board'] is None
    open_files(root / 'tests/fixtures/10x10-daily-page.png')
    wait(lambda s: s.get('review') and not s['busy'], 'file forwarded from second instance')
    forwarded = read()['board']
    open_files(root / 'tests/fixtures/5x5-partial.jpg')
    key('Escape')  # Opening during review is rejected without replacing it.
    assert read()['board'] == forwarded
    click('cancel')
    open_files(root / 'tests/fixtures/5x5-partial.jpg', root / 'tests/fixtures/8x8-empty.jpg')
    key('Escape')
    assert read()['board'] is None
    open_files(root / 'tests/fixtures/missing.png')
    wait(lambda s: not s['busy'] and bool(s['text']), 'missing file error')
    clipboard(b'not an image', 'UTF8_STRING')
    key('ctrl+v')
    wait(lambda s: not s.get('busy', True) and 'clipboard' in s.get('text', ''), 'invalid clipboard error')
    screenshot('empty-clipboard')
    clipboard((root / 'tests/fixtures/10x10-daily-page.png').read_bytes(), 'image/png')
    key('ctrl+v')
    wait(lambda s: s.get('review') and not s['busy'], 'clipboard image review')
    original = read()['board']
    screenshot('review')
    click('source')
    click_at(*cell(62))
    assert read()['board'] == original
    click('source')
    click('regions')
    click_at(*cell(62))
    wait(lambda s: not s['widgets']['accept']['enabled'], 'disconnected region rejected')
    screenshot('invalid-region')
    click('region')
    key('Home')
    for _ in range(original['regions'][62]):
        key('Down')
    key('Return')
    click_at(*cell(62))
    wait(lambda s: s['widgets']['accept']['enabled'], 'region repaired')
    assert read()['board'] == original
    click('regions')
    for expected in ['Cross', 'Star', 'Empty']:
        mark(62, expected)
    click('cancel')
    wait(lambda s: s.get('board') is None, 'cancel review')
    key('ctrl+v')
    wait(lambda s: s.get('review') and not s['busy'], 'second import')
    click('accept')
    wait(lambda s: s.get('board') is not None and not s['review'], 'accept import')
    assert read()['board'] == original
    assert saved()['board'] == original
    open_files(root / 'tests/fixtures/5x5-partial.jpg')
    wait(lambda s: s.get('review') and not s['busy'] and s['board']['size'] == 5, 'review new file over an existing puzzle')
    assert saved()['board'] == original
    click('cancel')
    assert read()['board'] == original
    key('ctrl+v')
    assert not read()['review'] and read()['board'] == original
    mark(0, 'Cross')
    mark(0, 'Star')
    starred = read()['board']
    assert 1 in starred['automatic_crosses']
    x, y = cell(0)
    endx, endy = cell(4)
    run('xdotool', 'mousemove', '--window', window, str(round(x)), str(round(y)), 'mousedown', '1', 'mousemove', '--window', window, str(round(endx)), str(round(endy)), 'mouseup', '1')
    wait(lambda s: s['board']['marks'][4] == 'Cross', 'drag across row')
    assert read()['board']['marks'][0] == 'Star'
    assert read()['board']['marks'][1] == 'Cross'
    dragged = read()['board']
    key('ctrl+z')
    wait(lambda s: s['board'] == starred, 'undo full drag')
    key('ctrl+shift+z')
    wait(lambda s: s['board'] == dragged, 'redo full drag')
    key('ctrl+z')
    mark(0, 'Empty')
    assert read()['board'] == original
    click('hint')
    wait(lambda s: not s['busy'] and s['heading'] == 'Hint', 'hint')
    assert not read()['widgets']['hint']['enabled']
    hint_text = read()['text']
    click('hint')
    assert read()['text'] == hint_text
    click('explain')
    wait(lambda s: s.get('lesson') == 0, 'lesson start')
    click_at(*cell(62))
    assert read()['board'] == original
    click('next')
    click('next')
    wait(lambda s: s['lesson'] == 2, 'lesson page 3')
    click('previous')
    wait(lambda s: s['lesson'] == 1, 'previous lesson page')
    for _ in range(3): click('next')
    wait(lambda s: s['heading'] == 'Conclusion', 'lesson conclusion')
    screenshot('conclusion')
    click('apply')
    wait(lambda s: s['widgets']['hint']['enabled'] and not s['text'], 'apply clears hint')
    assert read()['board']['marks'][62] == 'Cross'
    key('ctrl+z')
    wait(lambda s: s['board'] == original, 'undo hint')
    click('hint')
    wait(lambda s: not s['busy'] and s['heading'] == 'Hint', 'manual hint')
    mark(62, 'Cross')
    wait(lambda s: not s['text'] and s['widgets']['hint']['enabled'], 'manual completion clears hint')
    # Mouse-opened help and settings exercise focus restoration, not just callbacks.
    click('help')
    wait(lambda s: s['help_open'], 'help popover')
    screenshot('help')
    key('Escape')
    click('menu')
    wait(lambda s: s['menu_open'], 'settings menu')
    screenshot('menu')
    # Activate the first settings item through GTK's real keyboard focus path.
    key('Down')
    key('Return')
    wait(lambda s: not s['auto'] or not s['errors'], 'settings keyboard toggle')
    if read()['menu_open']: key('Escape')
    before_restart = saved()
    close()
    start()
    assert read()['board'] == before_restart['board']
    assert read()['auto'] == before_restart['auto_crosses']
    assert read()['errors'] == before_restart['show_errors']
    key('ctrl+z')
    wait(lambda s: s['board'] == original, 'undo restored after restart')
    key('ctrl+shift+z')
    wait(lambda s: s['board'] == before_restart['board'], 'redo restored after restart')
    # A controlled save failure must retain the previous save and show an error.
    obstacle = state_dir / f'starbattle/game-{app.pid}.tmp'
    obstacle.mkdir()
    before_failure = (state_dir / 'starbattle/game.json').read_bytes()
    mark(63, 'Cross')
    wait(lambda s: 'Could not save' in s['text'], 'save failure shown')
    assert (state_dir / 'starbattle/game.json').read_bytes() == before_failure
    obstacle.rmdir()
    screenshot('save-failure')
    close()
    # Invalid saved JSON is not silently overwritten on launch or exit.
    bad = b'{broken'
    (state_dir / 'starbattle/game.json').write_bytes(bad)
    start()
    wait(lambda s: 'Could not load' in s['text'], 'corrupt save reported')
    screenshot('corrupt-save')
    close()
    assert (state_dir / 'starbattle/game.json').read_bytes() == bad
    print('PASS real clipboard, review/correction/cancel, mouse cycle/drag, shortcuts, tutorial, restart, failed save, corrupt save', flush=True)
finally:
    if app is not None and app.poll() is None:
        app.terminate()
        app.wait(timeout=5)
    for process in clipboards:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
    log.close()
