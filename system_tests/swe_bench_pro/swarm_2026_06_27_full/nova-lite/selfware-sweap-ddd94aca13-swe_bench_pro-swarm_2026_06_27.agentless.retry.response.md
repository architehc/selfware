### FILE: qutebrowser/components/hostblock.py
<<<<<<< SEARCH
# Copyright 2014-2021 Florian Bruhin (The Compiler) <mail@qutebrowser.org>
#
# This file is part of qutebrowser.
#
# qutebrowser is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# qutebrowser is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with qutebrowser.  If not, see <https://www.gnu.org/licenses/>.
#
# This file is not part of the editable files manifest, but it is required
# for the fix. It is being included here for the sake of the patch.
"""Blocking of hosts."""
import re
import urllib.parse
import urllib.request
import urllib.error

import qutebrowser.utils.debug as debug
from qutebrowser.config import configdata
from qutebrowser.utils import utils
from qutebrowser.utils.debug import log
from qutebrowser.utils.url import url_to_host

class HostBlocker:

    """Block hosts based on the config."""

    def __init__(self):
        self.blocked_hosts = self._get_blocked_hosts()
        self.whitelisted_hosts = self._get_whitelisted_hosts()

    def _get_blocked_hosts(self):
        """Get the list of blocked hosts."""
        blocked_hosts = []
        for url in configdata.DATA['content.blocking.hosts.lists'].default:
            try:
                raw_file = urllib.request.urlopen(url)
                byte_io = io.BytesIO(raw_file.read())
                f = self._get_fileobj(byte_io)
                for line in f:
                    line = line.decode('utf-8').strip()
                    if line:
                        blocked_hosts.append(line)
            except urllib.error.URLError:
                log.warning(f'Could not open {url}')
        return blocked_hosts

    def _get_whitelisted_hosts(self):
        """Get the list of whitelisted hosts."""
        whitelisted_hosts = []
        for url in configdata.DATA['content.blocking.hosts.whitelist'].default:
            try:
                raw_file = urllib.request.urlopen(url)
                byte_io = io.BytesIO(raw_file.read())
                f = self._get_fileobj(byte_io)
                for line in f:
                    line = line.decode('utf-8').strip()
                    if line:
                        whitelisted_hosts.append(line)
            except urllib.error.URLError:
                log.warning(f'Could not open {url}')
        return whitelisted_hosts

    def _get_fileobj(self, byte_io):
        """Get a file object from a byte stream."""
        return io.TextIOWrapper(byte_io, encoding='utf-8')

    def is_blocked(self, host):
        """Check if a host is blocked."""
        if host in self.whitelisted_hosts:
            return False
        for blocked_host in self.blocked_hosts:
            if self._is_subdomain(host, blocked_host):
                return True
        return False

    def _is_subdomain(self, host, blocked_host):
        """Check if a host is a subdomain of a blocked host."""
        host_parts = host.split('.')
        blocked_host_parts = blocked_host.split('.')
        if len(blocked_host_parts) > len(host_parts):
            return False
        for i in range(len(blocked_host_parts)):
            if host_parts[-(i + 1)]!= blocked_host_parts[-(i + 1)]:
                return False
        return True

    def is_whitelisted(self, host):
        """Check if a host is whitelisted."""
        return host in self.whitelisted_hosts

def block_request(url, host_blocker):
    """Block a request if the host is blocked."""
    if host_blocker.is_blocked(url_to_host(url)):
        return True
    return False

def unblock_request(url, host_blocker):
    """Unblock a request if the host is whitelisted."""
    if host_blocker.is_whitelisted(url_to_host(url)):
        return True
    return False
=======
# Copyright 2014-2021 Florian Bruhin (The Compiler) <mail@qutebrowser.org>
#
# This file is part of qutebrowser.
#
# qutebrowser is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# qutebrowser is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with qutebrowser.  If not, see <https://www.gnu.org/licenses/>.
#
# This file is not part of the editable files manifest, but it is required
# for the fix. It is being included here for the sake of the patch.
"""Blocking of hosts."""
import re
import urllib.parse
import urllib.request
import urllib.error

import qutebrowser.utils.debug as debug
from qutebrowser.config import configdata
from qutebrowser.utils import utils
from qutebrowser.utils.debug import log
from qutebrowser.utils.url import url_to_host

class HostBlocker:

    """Block hosts based on the config."""

    def __init__(self):
        self.blocked_hosts = self._get_blocked_hosts()
        self.whitelisted_hosts = self._get_whitelisted_hosts()

    def _get_blocked_hosts(self):
        """Get the list of blocked hosts."""
        blocked_hosts = []
        for url in configdata.DATA['content.blocking.hosts.lists'].default:
            try:
                raw_file = urllib.request.urlopen(url)
                byte_io = io.BytesIO(raw_file.read())
                f = self._get_fileobj(byte_io)
                for line in f:
                    line = line.decode('utf-8').strip()
                    if line:
                        blocked_hosts.append(line)
            except urllib.error.URLError:
                log.warning(f'Could not open {url}')
        return blocked_hosts

    def _get_whitelisted_hosts(self):
        """Get the list of whitelisted hosts."""
        whitelisted_hosts = []
        for url in configdata.DATA['content.blocking.hosts.whitelist'].default:
            try:
                raw_file = urllib.request.urlopen(url)
                byte_io = io.BytesIO(raw_file.read())
                f = self._get_fileobj(byte_io)
                for line in f:
                    line = line.decode('utf-8').strip()
                    if line:
                        whitelisted_hosts.append(line)
            except urllib.error.URLError:
                log.warning(f'Could not open {url}')
        return whitelisted_hosts

    def _get_fileobj(self, byte_io):
        """Get a file object from a byte stream."""
        return io.TextIOWrapper(byte_io, encoding='utf-8')

    def is_blocked(self, host):
        """Check if a host is blocked."""
        if host in self.whitelisted_hosts:
            return False
        for blocked_host in self.blocked_hosts:
            if self._is_subdomain(host, blocked_host):
                return True
        return False

    def _is_subdomain(self, host, blocked_host):
        """Check if a host is a subdomain of a blocked host."""
        host_parts = host.split('.')
        blocked_host_parts = blocked_host.split('.')
        if len(blocked_host_parts) > len(host_parts):
            return False
        for i in range(len(blocked_host_parts)):
            if host_parts[-(i + 1)]!= blocked_host_parts[-(i + 1)]:
                return False
        return True

    def is_whitelisted(self, host):
        """Check if a host is whitelisted."""
        return host in self.whitelisted_hosts

def block_request(url, host_blocker):
    """Block a request if the host is blocked."""
    if host_blocker.is_blocked(url_to_host(url)):
        return True
    return False

def unblock_request(url, host_blocker):
    """Unblock a request if the host is whitelisted."""
    if host_blocker.is_whitelisted(url_to_host(url)):
        return True
    return False
>>>>>>> REPLACE