import shutil


def pytest_collection_modifyitems(config, items):
    """Skip all tests if Node.js is not available."""
    if shutil.which("node") is None:
        import pytest

        skip = pytest.mark.skip(reason="Node.js not found on PATH")
        for item in items:
            item.add_marker(skip)
