#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <X11/Xlib.h>

int main(void)
{
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) {
        fprintf(stderr, "hello-gui: cannot open display (no X11/Wayland?)\n");
        return 1;
    }

    int screen = DefaultScreen(dpy);
    Window win = XCreateSimpleWindow(
        dpy, RootWindow(dpy, screen),
        100, 100, 400, 200, 1,
        BlackPixel(dpy, screen), WhitePixel(dpy, screen));

    XSelectInput(dpy, win, ExposureMask | StructureNotifyMask);
    XMapWindow(dpy, win);

    GC gc = XCreateGC(dpy, win, 0, NULL);
    XSetForeground(dpy, gc, BlackPixel(dpy, screen));

    Atom wm_delete = XInternAtom(dpy, "WM_DELETE_WINDOW", False);
    XSetWMProtocols(dpy, win, &wm_delete, 1);

    int running = 1;
    XEvent ev;
    long start = 0;

    while (running) {
        if (XPending(dpy) > 0) {
            XNextEvent(dpy, &ev);
            if (ev.type == Expose && ev.xexpose.count == 0) {
                XDrawString(dpy, win, gc, 50, 80,
                    "Hello from Nexpack!", 20);
                XDrawString(dpy, win, gc, 50, 110,
                    "This app runs inside the sandbox.", 34);
                XDrawString(dpy, win, gc, 50, 140,
                    "PID: ", 5);
                char pid[16];
                snprintf(pid, sizeof(pid), "%d", getpid());
                XDrawString(dpy, win, gc, 80, 140, pid, strlen(pid));
            }
            if (ev.type == ClientMessage &&
                (Atom)ev.xclient.data.l[0] == wm_delete) {
                running = 0;
            }
        }

        if (start == 0) start = time(NULL);
        if (time(NULL) - start >= 3) running = 0;

        usleep(50000);
    }

    XFreeGC(dpy, gc);
    XDestroyWindow(dpy, win);
    XCloseDisplay(dpy);

    printf("hello-gui: completed successfully\n");
    return 0;
}
