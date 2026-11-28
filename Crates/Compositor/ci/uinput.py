import fcntl, os, struct, sys, time

UI_DEV_CREATE = 0x5501
UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_RELBIT = 0x40045566
EV_SYN, EV_KEY, EV_REL = 0, 1, 2
KEYS = {"bs": 14, "ctrl": 29, "alt": 56, "a": 30, "left": 272, "right": 273}


def device(name, keys, rels):
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for key in keys:
        fcntl.ioctl(fd, UI_SET_KEYBIT, key)
    if rels:
        fcntl.ioctl(fd, UI_SET_EVBIT, EV_REL)
        for rel in rels:
            fcntl.ioctl(fd, UI_SET_RELBIT, rel)
    os.write(fd, struct.pack("80sHHHHi" + "64i" * 4, name, 3, 1, 1, 1, 0, *([0] * 256)))
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd


def emit(fd, kind, code, value):
    now = time.time()
    os.write(fd, struct.pack("llHHi", int(now), int((now % 1) * 1e6), kind, code, value))


def sync(fd):
    emit(fd, EV_SYN, 0, 0)


keyboard = device(b"arxumbra-keyboard", range(1, 249), [])
mouse = device(b"arxumbra-mouse", [272, 273], [0, 1])
print("ready", flush=True)

for line in sys.stdin:
    words = line.split()
    if not words:
        continue
    if words[0] == "key":
        codes = [KEYS[word] for word in words[1:]]
        for code in codes:
            emit(keyboard, EV_KEY, code, 1)
            sync(keyboard)
            time.sleep(0.05)
        time.sleep(0.1)
        for code in reversed(codes):
            emit(keyboard, EV_KEY, code, 0)
            sync(keyboard)
            time.sleep(0.05)
    elif words[0] == "move":
        dx, dy, steps = int(words[1]), int(words[2]), int(words[3])
        for _ in range(steps):
            emit(mouse, EV_REL, 0, dx)
            emit(mouse, EV_REL, 1, dy)
            sync(mouse)
            time.sleep(0.01)
    elif words[0] == "click":
        code = KEYS[words[1]]
        emit(mouse, EV_KEY, code, 1)
        sync(mouse)
        time.sleep(0.05)
        emit(mouse, EV_KEY, code, 0)
        sync(mouse)
    elif words[0] == "sleep":
        time.sleep(float(words[1]))
    print("done", line.strip(), flush=True)
