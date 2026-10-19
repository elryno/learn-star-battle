#!/usr/bin/env python3
"""Use actual Wayland data transfer and pointer events on a private Sway display."""
import json
import os
import select
from pathlib import Path
import subprocess as sp
import sys
import time

state, output = map(Path, sys.argv[1:])
root = Path.cwd()
observation = output / 'external.json'
log = (output / 'app.log').open('w')
results = (output / 'results.txt').open('w')
app = None
providers = []
keyboard = sp.Popen(['wtype', '-s', '600000'], stdout=log, stderr=log)
mouse = sp.Popen([str(output / 'pointer')], stdin=sp.PIPE, stdout=sp.PIPE, stderr=log, text=True, bufsize=1)

def pointer_reply(expected):
    assert select.select([mouse.stdout], [], [], 5)[0], 'Pointer did not respond'
    assert mouse.stdout.readline().strip() == expected

pointer_reply('ready')

def mouse_command(text):
    mouse.stdin.write(text + '\n')
    mouse.stdin.flush()
    pointer_reply('ok')

def run(*args):
    return sp.check_output(args, text=True, stderr=sp.STDOUT).strip()

def read():
    try:
        return json.loads(observation.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}

def wait(predicate, description):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        current = read()
        if predicate(current): return current
        if app is not None and app.poll() is not None:
            raise AssertionError(f'App exited during {description}: {app.returncode}')
        time.sleep(.1)
    run('grim', str(output / 'failure.png'))
    (output / 'failure-tree.json').write_text(run('swaymsg', '-r', '-t', 'get_tree'))
    (output / 'failure-seats.json').write_text(run('swaymsg', '-r', '-t', 'get_seats'))
    raise AssertionError(f'Timed out: {description}; {read()}')

def command(text):
    result = json.loads(run('swaymsg', '-r', text))
    assert all(item.get('success') for item in result), result

def window_rect():
    def find(node):
        if node.get('app_id') == 'io.github.starbattle.Solver': return node['rect']
        for child in node.get('nodes', []) + node.get('floating_nodes', []):
            result = find(child)
            if result: return result
    return find(json.loads(run('swaymsg', '-r', '-t', 'get_tree')))

def pointer(x, y):
    rect = window_rect()
    screen = json.loads(run('swaymsg', '-r', '-t', 'get_outputs'))[0]['rect']
    mouse_command(f'move {round(rect["x"] + x)} {round(rect["y"] + y)} {screen["width"]} {screen["height"]}')

def click_at(x, y):
    pointer(x, y)
    mouse_command('press')
    mouse_command('release')
    time.sleep(.15)

def settled(widget_name=None):
    # State notifications can precede GTK's next layout allocation. In
    # particular, import controls move after the image finishes loading.
    # Wait for stable geometry before choosing coordinates for real input.
    previous, stable_since = None, time.monotonic()
    deadline = stable_since + 15
    while time.monotonic() < deadline:
        current = read()
        geometry = (current.get('grid'), current.get('widgets'), window_rect())
        widget = current.get('widgets', {}).get(widget_name, {})
        allocated = widget_name is None or (widget.get('visible') and widget.get('allocated'))
        if geometry != previous or not allocated:
            previous, stable_since = geometry, time.monotonic()
        elif time.monotonic() - stable_since >= .3:
            return current
        time.sleep(.1)
    raise AssertionError('Window layout did not settle')

def click(name):
    widget = settled(name)['widgets'][name]
    assert widget['visible'] and widget['enabled'], name
    click_at(widget['x'], widget['y'])

def cell(i):
    s = settled()
    g, n = s['grid'], s['board']['size']
    return g['x'] + (i % n + .5)*g['cell'], g['y'] + (i // n + .5)*g['cell']

def copy(data, mime):
    process = sp.Popen(['wl-copy', '--foreground', '--type', mime], stdin=sp.PIPE, stdout=sp.DEVNULL, stderr=log)
    providers.append(process)
    process.stdin.write(data)
    process.stdin.close()
    time.sleep(.2)

def paste():
    run('wtype', '-s', '250', '-M', 'ctrl', '-k', 'v', '-m', 'ctrl', '-s', '100')

try:
    for scale in map(float, os.environ.get('STAR_BATTLE_TEST_SCALES', '1 1.25 1.5 2').split()):
        command(f'output HEADLESS-1 scale {scale}')
        actual = json.loads(run('swaymsg', '-r', '-t', 'get_outputs'))[0]['scale']
        assert abs(actual - scale) < .01, actual
        observation.unlink(missing_ok=True)
        env = dict(os.environ, XDG_DATA_HOME=str(state / str(scale)), STAR_BATTLE_CAPTURE_SCENARIO='external', STAR_BATTLE_CAPTURE_PATH=str(output / 'external.png'))
        env.pop('STAR_BATTLE_TEST_BOARD', None)
        app = sp.Popen([str(root / 'target/debug/star-battle-solver')], env=env, stdout=log, stderr=log)
        wait(lambda s: bool(s.get('widgets')), 'startup')
        screen = json.loads(run('swaymsg', '-r', '-t', 'get_outputs'))[0]['rect']
        command('[app_id="io.github.starbattle.Solver"] resize set '
                f'width {min(1040, screen["width"] - 40)} px '
                f'height {min(740, screen["height"] - 40)} px')
        command('[app_id="io.github.starbattle.Solver"] move position center')
        command('[app_id="io.github.starbattle.Solver"] focus')
        copy(b'not an image', 'text/plain;charset=utf-8')
        paste()
        wait(lambda s: not s.get('busy', True) and 'clipboard' in s.get('text', ''), 'nonimage clipboard')
        copy((root / 'tests/fixtures/10x10-daily-page.png').read_bytes(), 'image/png')
        paste()
        wait(lambda s: s.get('review') and not s['busy'], 'Wayland clipboard PNG import')
        expected = json.loads((root / 'tests/fixtures/10x10-daily-page.json').read_text())
        original = read()['board']
        for field in ['size', 'marks', 'regions', 'region_colors']:
            assert original[field] == expected[field], field
        click('source')
        wait(lambda s: s['source'], 'show source')
        click_at(*cell(0))
        assert read()['board'] == original
        click('source')
        wait(lambda s: not s['source'], 'show recreated board')
        click('accept')
        wait(lambda s: not s['review'], 'accept')
        assert read()['board'] == original, 'Accept changed the imported board'
        assert not read()['widgets']['undo']['enabled'], 'Unexpected import history'
        click_at(*cell(0))
        wait(lambda s: s['board']['marks'][0] == 'Cross', 'scaled cell hit')
        assert read()['widgets']['undo']['enabled'], 'Cell click was not committed'
        pointer(*cell(1))
        mouse_command('press')
        pointer(*cell(4))
        mouse_command('release')
        wait(lambda s: s['board']['marks'][4] == 'Cross', 'scaled drag')
        run('wtype', '-s', '250', '-M', 'ctrl', '-k', 'z', '-m', 'ctrl', '-s', '100')
        wait(lambda s: s['board']['marks'][4] == original['marks'][4], 'undo drag')
        run('wtype', '-s', '250', '-M', 'ctrl', '-k', 'z', '-m', 'ctrl', '-s', '100')
        wait(lambda s: s['board'] == original, 'undo click')
        click('hint')
        wait(lambda s: not s['busy'] and s['heading'] == 'Hint', 'hint')
        click('explain')
        wait(lambda s: s.get('lesson') == 0, 'lesson')
        click_at(*cell(62))
        assert read()['board'] == original
        for _ in range(4): click('next')
        wait(lambda s: s['heading'] == 'Conclusion', 'conclusion')
        run('grim', str(output / f'scale-{scale}.png'))
        click('apply')
        wait(lambda s: s['board']['marks'][62] == 'Cross' and not s['text'], 'apply')
        command('[app_id="io.github.starbattle.Solver"] kill')
        assert app.wait(timeout=5) == 0
        results.write(f'PASS Wayland clipboard, pointer, tutorial at {scale * 100:g}% scale\n')
        results.flush()
finally:
    mouse.stdin.close()
    mouse.wait(timeout=5)
    keyboard.terminate()
    keyboard.wait(timeout=5)
    if app is not None and app.poll() is None:
        app.terminate()
        app.wait(timeout=5)
    for process in providers:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
    log.close()
    results.close()
