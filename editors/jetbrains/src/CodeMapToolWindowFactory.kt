package com.selfware

import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefBrowser
import java.io.File

/**
 * Selfware Code Map -- JetBrains tool window scaffold.
 *
 * Creates an embedded JCEF (Chromium) panel that renders the code graph
 * using the same HTML/Canvas visualisation shared across editors.
 * Communication between Kotlin and the webview happens via a JS bridge.
 */
class CodeMapToolWindowFactory : ToolWindowFactory {

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val panel = CodeMapPanel(project)
        val content = ContentFactory.getInstance().createContent(panel.browser.component, "Code Map", false)
        toolWindow.contentManager.addContent(content)
    }
}

/**
 * Wraps a JCEF browser that loads codegraph.json and renders it.
 */
class CodeMapPanel(private val project: Project) {

    val browser: JBCefBrowser = JBCefBrowser()

    init {
        loadGraph()
    }

    private fun loadGraph() {
        val basePath = project.basePath ?: return
        val graphFile = File(basePath, "codegraph.json")

        if (!graphFile.exists()) {
            browser.loadHTML("<html><body><p>codegraph.json not found. Run <code>cargo run --bin codegraph</code> first.</p></body></html>")
            return
        }

        val graphJson = graphFile.readText()

        // Inject the graph data into a minimal HTML page.
        // A real implementation would load the shared webview assets from
        // the vscode-selfware/media/ directory or a bundled resource.
        val html = """
            <!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>Selfware Code Map</title></head>
            <body>
                <h3>Selfware Code Map</h3>
                <pre id="graph" style="font-size:12px; overflow:auto; max-height:90vh;"></pre>
                <script>
                    const graph = $graphJson;
                    const el = document.getElementById('graph');
                    el.textContent = JSON.stringify(graph, null, 2);

                    // TODO: Replace with canvas/D3 rendering from the shared
                    // webview assets once they are extracted into a reusable
                    // module.
                </script>
            </body>
            </html>
        """.trimIndent()

        browser.loadHTML(html)
    }
}
