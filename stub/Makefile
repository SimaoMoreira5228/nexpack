CC ?= gcc
CFLAGS = -nostdlib -static -Os -s \
	 -fno-stack-protector \
	 -fno-unwind-tables \
	 -fno-asynchronous-unwind-tables \
	 -fomit-frame-pointer \
	 -ffreestanding \
	 -Wall -Wextra -Werror

LDFLAGS = -Wl,--gc-sections -Wl,-z,norelro -Wl,--build-id=none

TARGET = stub

all: $(TARGET)

$(TARGET): stub.c
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $^

size: $(TARGET)
	@echo "Stub size:"
	@size $(TARGET)
	@ls -lh $(TARGET)

clean:
	rm -f $(TARGET)

.PHONY: all size clean
