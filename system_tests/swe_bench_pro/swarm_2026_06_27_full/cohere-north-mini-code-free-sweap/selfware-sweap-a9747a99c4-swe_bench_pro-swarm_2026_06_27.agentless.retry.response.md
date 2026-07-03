I'll start by understanding the issue. The problem is that the `uri` module uses `.netrc` to overwrite the Authorization header even if specified. The user wants a new `use_netrc` parameter (defaulting to `true`) to control this behavior. The fix involves modifying several files:

1. `lib/ansible/module_utils/urls.py` - `Request.__init__` and `Request.open` need to expose a parameter for `.netrc` usage.
2. `lib/ansible/module_utils/urls.py` - `open_url` needs to define a parameter for `.netrc` handling and forward it to `Request`.
3. `lib/ansible/module_utils/urls.py` - `fetch_url` needs to accept a parameter that signals whether `.netrc` is used and pass it to `open_url`.
4. `lib/ansible/module_utils/urls.py` - `url_get` should define a parameter for `.netrc` usage and forward it to `fetch_url`.
5. `lib/ansible/modules/get_url.py` - The main flow of `get_url` must retrieve the `.netrc` setting from module arguments and consistently forward it to `url_get` for both primary download and checksum retrieval.
6. `lib/ansible/modules/uri.py` - The `uri` function requires a parameter controlling `.netrc` behavior and should propagate it to the lower-level request call.
7. `lib/ansible/plugins/lookup/url.py` - The `run` method of the `url` lookup plugin must respect the configured `.netrc` option and forward it to the underlying request logic.

Additionally, we need to create test files for integration tests.

Let's start by examining the current state of these files to understand the existing code and where to make changes.

First, let's look at `lib/ansible/module_utils/urls.py` to see the `Request` class and `open_url`, `fetch_url`, `url_get` functions.