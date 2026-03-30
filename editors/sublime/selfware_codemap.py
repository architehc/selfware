"""
Selfware Code Map -- Sublime Text plugin scaffold.

Install: copy this file to Packages/User/selfware_codemap.py

Commands (available in the command palette):
  - selfware_open_codemap   — show graph nodes in a quick panel
  - selfware_context_add    — add current file's node to context
  - selfware_inspect        — show node details in an HTML popup
"""

import json
import os

import sublime
import sublime_plugin


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

_graph = None        # parsed codegraph.json
_context = {}        # node_id -> node dict
_budget = 128000     # token budget


def _graph_path(window):
    """Return the path to codegraph.json in the first open folder."""
    folders = window.folders()
    if not folders:
        return None
    return os.path.join(folders[0], "codegraph.json")


def _load_graph(window):
    global _graph
    path = _graph_path(window)
    if not path or not os.path.exists(path):
        sublime.status_message("selfware: codegraph.json not found")
        return []
    with open(path, "r") as f:
        _graph = json.load(f)
    return _graph.get("nodes", [])


def _nodes(window):
    if _graph is None:
        return _load_graph(window)
    return _graph.get("nodes", [])


def _context_tokens():
    return sum(n.get("tokens", 0) for n in _context.values())


def _update_status(view):
    used = _context_tokens()
    view.set_status("selfware", "ctx {}/{} tok".format(used, _budget))


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

class SelfwareOpenCodemapCommand(sublime_plugin.WindowCommand):
    """Open a quick panel listing all graph nodes."""

    def run(self):
        nodes = _nodes(self.window)
        if not nodes:
            return

        self._nodes = nodes
        items = [
            "[{}] {}  ({} tok)".format(
                n.get("kind", "?"), n.get("id", "?"), n.get("tokens", 0)
            )
            for n in nodes
        ]
        self.window.show_quick_panel(items, self._on_select)

    def _on_select(self, idx):
        if idx < 0:
            return
        node = self._nodes[idx]
        _show_inspect_popup(self.window.active_view(), node)


class SelfwareContextAddCommand(sublime_plugin.TextCommand):
    """Add the current file's graph node to the context set."""

    def run(self, edit):
        path = self.view.file_name()
        if not path:
            return
        for node in _nodes(self.view.window()):
            if node.get("file") and path.endswith(node["file"]):
                _context[node["id"]] = node
                sublime.status_message("+ {} ({} tok)".format(
                    node["id"], node.get("tokens", 0)))
                _update_status(self.view)
                return
        sublime.status_message("selfware: no graph node for this file")


class SelfwareInspectCommand(sublime_plugin.TextCommand):
    """Show an HTML popup with details for the current file's node."""

    def run(self, edit):
        path = self.view.file_name()
        if not path:
            return
        for node in _nodes(self.view.window()):
            if node.get("file") and path.endswith(node["file"]):
                _show_inspect_popup(self.view, node)
                return
        sublime.status_message("selfware: no graph node for this file")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _show_inspect_popup(view, node):
    deps = node.get("deps", [])
    deps_html = "".join("<li>{}</li>".format(d) for d in deps) or "<li>none</li>"
    html = """
    <body>
        <h4>{id}</h4>
        <p>Kind: {kind}<br>File: {file}<br>Tokens: {tokens}</p>
        <p>Dependencies:</p>
        <ul>{deps}</ul>
    </body>
    """.format(
        id=node.get("id", "?"),
        kind=node.get("kind", "?"),
        file=node.get("file", "?"),
        tokens=node.get("tokens", 0),
        deps=deps_html,
    )
    view.show_popup(html, max_width=500, max_height=400)
