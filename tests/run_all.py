#!/usr/bin/env python3
import os, sys, importlib, pkgutil

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def main():
    passed = 0
    failed = 0
    total = 0

    for _, name, _ in pkgutil.iter_modules([HERE]):
        if name.startswith("test_") and name != "test_all":
            mod = importlib.import_module(name)
            total += 1
            try:
                if hasattr(mod, "test"):
                    mod.test()
            except Exception as e:
                print(f"  CRASH {name}: {e}")
                failed += 1

    import conftest

    p = conftest.PASSED
    f = conftest.FAILED

    print(f"\n{'=' * 50}")
    print(f"{p} assertions passed, {f} failed, across {total} test files")

    return 0 if f == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
