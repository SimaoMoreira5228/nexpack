CC ?= gcc
CFLAGS = -Os -Wall -Wextra
LDFLAGS_STATIC = -static -s
LDFLAGS_DYNAMIC = -s

TARGETS = simple probe hello-gui

all: $(TARGETS)

simple: CFLAGS += -ffreestanding
simple: LDFLAGS = -static -s -nostdlib -ffreestanding \
	-fno-stack-protector -fno-unwind-tables \
	-fno-asynchronous-unwind-tables -fomit-frame-pointer \
	-Wl,--gc-sections -Wl,-z,norelro -Wl,--build-id=none
simple: simple.c
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

probe: probe.c
	$(CC) $(CFLAGS) $(LDFLAGS_STATIC) -o $@ $^

hello-gui: hello-gui.c
	@X11_CFLAGS=$$(pkg-config --cflags x11 2>/dev/null); \
	X11_LIBS=$$(pkg-config --libs x11 2>/dev/null); \
	if [ -n "$$X11_CFLAGS" ] && $(CC) $(CFLAGS) $$X11_CFLAGS -c hello-gui.c -o /dev/null 2>/dev/null; then \
		echo "  CC  $@"; \
		$(CC) $(CFLAGS) $(LDFLAGS_DYNAMIC) $$X11_CFLAGS -o $@ $^ $$X11_LIBS; \
	else \
		echo "  SKIP $@ (X11 not available)"; \
	fi

size: $(TARGETS)
	@size $(TARGETS)
	@ls -lh $(TARGETS)

clean:
	rm -f $(TARGETS)

.PHONY: all size clean
