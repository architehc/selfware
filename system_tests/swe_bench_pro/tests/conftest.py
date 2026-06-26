"""Pytest fixtures and import shims for the SWE-bench Pro harness tests."""

import sys
from types import ModuleType

# The harness scripts import ``tomli_w`` at module level, but it may not be
# installed in the test environment. Provide a harmless stub so prompt-building
# tests can import ``run_selfware`` without requiring the full runtime deps.
if "tomli_w" not in sys.modules:
    stub = ModuleType("tomli_w")
    stub.dump = lambda obj, fp, **kwargs: None  # type: ignore[assignment]
    sys.modules["tomli_w"] = stub
