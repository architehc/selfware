import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { ContextManager, CodeGraph } from './contextManager';
import { getWebviewContent } from './webview';

let contextManager: ContextManager;
let codeMapPanel: vscode.WebviewPanel | undefined;

export function activate(context: vscode.ExtensionContext) {
    contextManager = new ContextManager(1_000_000);

    // Try to load codegraph.json from workspace root
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (workspaceFolders) {
        const graphPath = path.join(workspaceFolders[0].uri.fsPath, 'codegraph.json');
        loadGraph(graphPath);
    }

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('selfware.openCodeMap', () => openCodeMap(context)),
        vscode.commands.registerCommand('selfware.contextAdd', () => contextAddPrompt()),
        vscode.commands.registerCommand('selfware.contextRemove', () => contextRemovePrompt()),
        vscode.commands.registerCommand('selfware.contextClear', () => {
            contextManager.clearContext();
            updateWebview();
            vscode.window.showInformationMessage('Context cleared.');
        }),
        vscode.commands.registerCommand('selfware.inspect', () => inspectPrompt()),
        vscode.commands.registerCommand('selfware.buildNew', () => buildNewPrompt()),
    );

    // Watch for codegraph.json changes
    if (workspaceFolders) {
        const watcher = vscode.workspace.createFileSystemWatcher(
            new vscode.RelativePattern(workspaceFolders[0], 'codegraph.json')
        );
        watcher.onDidChange(uri => loadGraph(uri.fsPath));
        watcher.onDidCreate(uri => loadGraph(uri.fsPath));
        context.subscriptions.push(watcher);
    }

    // Context tree data provider for the sidebar
    const contextProvider = new ContextTreeProvider(contextManager);
    context.subscriptions.push(
        vscode.window.registerTreeDataProvider('selfware.context', contextProvider)
    );
    contextManager.onChange(() => contextProvider.refresh());

    // Actions tree data provider
    const actionsProvider = new ActionsTreeProvider();
    context.subscriptions.push(
        vscode.window.registerTreeDataProvider('selfware.actions', actionsProvider)
    );
}

function loadGraph(graphPath: string): void {
    try {
        if (fs.existsSync(graphPath)) {
            const raw = fs.readFileSync(graphPath, 'utf-8');
            const graph: CodeGraph = JSON.parse(raw);
            contextManager.setGraph(graph);
            vscode.window.showInformationMessage(
                `Code map loaded: ${graph.nodes.length} nodes, ${graph.edges.length} edges`
            );
            updateWebview();
        }
    } catch (err) {
        vscode.window.showErrorMessage(`Failed to load codegraph.json: ${err}`);
    }
}

function openCodeMap(context: vscode.ExtensionContext): void {
    if (codeMapPanel) {
        codeMapPanel.reveal(vscode.ViewColumn.One);
        return;
    }

    codeMapPanel = vscode.window.createWebviewPanel(
        'selfwareCodeMap',
        'Selfware Code Map',
        vscode.ViewColumn.One,
        { enableScripts: true, retainContextWhenHidden: true }
    );

    updateWebview();

    // Handle messages from webview
    codeMapPanel.webview.onDidReceiveMessage(
        (message: { type: string; action: string; nodeId: string }) => {
            if (message.type === 'action') {
                handleWebviewAction(message.action, message.nodeId);
            }
        },
        undefined,
        context.subscriptions
    );

    codeMapPanel.onDidDispose(() => {
        codeMapPanel = undefined;
    });
}

function handleWebviewAction(action: string, nodeId: string): void {
    switch (action) {
        case 'ctx_add':
            if (!contextManager.addToContext(nodeId)) {
                const node = contextManager.findNode(nodeId);
                if (node && contextManager.isInContext(nodeId)) {
                    vscode.window.showInformationMessage(`${node.label} is already in context.`);
                } else {
                    vscode.window.showWarningMessage('Cannot add: node not found or budget exceeded.');
                }
            }
            updateWebview();
            break;
        case 'ctx_remove':
            contextManager.removeFromContext(nodeId);
            updateWebview();
            break;
        case 'inspect':
        case 'read_full':
        case 'read_skeleton':
        case 'alter':
        case 'build_new':
        case 'verify':
        case 'test':
        case 'ship':
        case 'git_diff': {
            const node = contextManager.findNode(nodeId);
            if (node) {
                vscode.window.showInformationMessage(
                    `Action "${action}" on ${node.label} (${node.tokens} tokens) -- integrate with agent loop.`
                );
            }
            break;
        }
    }
}

function updateWebview(): void {
    if (!codeMapPanel) {
        return;
    }
    const graph = contextManager.getGraph();
    if (!graph) {
        codeMapPanel.webview.html = `<!DOCTYPE html><html><body style="background:#1e1e1e;color:#ccc;padding:40px;font-family:sans-serif;">
            <h2>No code graph loaded</h2>
            <p>Create a <code>codegraph.json</code> in your workspace root.</p>
            <p>Expected format: <code>{ "nodes": [...], "edges": [...] }</code></p>
            <p>Each node: <code>{ "id", "label", "kind", "tokens", "path?", "fusion_group?", "children?", "deps?" }</code></p>
            <p>Each edge: <code>{ "source", "target", "kind" }</code></p>
        </body></html>`;
        return;
    }

    codeMapPanel.webview.html = getWebviewContent(
        codeMapPanel.webview,
        graph,
        contextManager.getContextNodeIds(),
        contextManager.getTokenBudget()
    );
}

async function contextAddPrompt(): Promise<void> {
    const graph = contextManager.getGraph();
    if (!graph) {
        vscode.window.showWarningMessage('No code graph loaded.');
        return;
    }
    const items = graph.nodes.map(n => ({
        label: n.label,
        description: `${n.kind} - ${n.tokens} tokens`,
        detail: n.path || n.id,
        nodeId: n.id,
    }));
    const picked = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select a node to add to context',
        matchOnDescription: true,
        matchOnDetail: true,
    });
    if (picked) {
        if (!contextManager.addToContext(picked.nodeId)) {
            vscode.window.showWarningMessage('Cannot add: already in context or budget exceeded.');
        } else {
            updateWebview();
        }
    }
}

async function contextRemovePrompt(): Promise<void> {
    const nodes = contextManager.getContextNodes();
    if (nodes.length === 0) {
        vscode.window.showInformationMessage('Context is empty.');
        return;
    }
    const items = nodes.map(n => ({
        label: n.label,
        description: `${n.kind} - ${n.tokens} tokens`,
        nodeId: n.id,
    }));
    const picked = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select a node to remove from context',
    });
    if (picked) {
        contextManager.removeFromContext(picked.nodeId);
        updateWebview();
    }
}

async function inspectPrompt(): Promise<void> {
    const graph = contextManager.getGraph();
    if (!graph) {
        vscode.window.showWarningMessage('No code graph loaded.');
        return;
    }
    const items = graph.nodes.map(n => ({
        label: n.label,
        description: `${n.kind} - ${n.tokens} tokens`,
        detail: n.path || n.id,
        nodeId: n.id,
    }));
    const picked = await vscode.window.showQuickPick(items, {
        placeHolder: 'Select a component to inspect',
        matchOnDescription: true,
        matchOnDetail: true,
    });
    if (picked) {
        const node = contextManager.findNode(picked.nodeId);
        if (node) {
            const actions = contextManager.suggestActions(picked.nodeId);
            const actionItems = actions.map(a => ({
                label: a.label,
                description: `${a.tokenCost} tokens`,
                detail: a.description,
                actionType: a.type,
            }));
            const actionPicked = await vscode.window.showQuickPick(actionItems, {
                placeHolder: `Actions for ${node.label}`,
            });
            if (actionPicked) {
                handleWebviewAction(actionPicked.actionType, picked.nodeId);
            }
        }
    }
}

async function buildNewPrompt(): Promise<void> {
    const name = await vscode.window.showInputBox({
        prompt: 'Name for the new component',
        placeHolder: 'my_new_module',
    });
    if (name) {
        vscode.window.showInformationMessage(
            `Build new component "${name}" -- integrate with agent loop.`
        );
    }
}

// Tree data providers

class ContextTreeProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData = new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    constructor(private cm: ContextManager) {}

    refresh(): void {
        this._onDidChangeTreeData.fire(undefined);
    }

    getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
        return element;
    }

    getChildren(): vscode.TreeItem[] {
        const budget = this.cm.getTokenBudget();
        const budgetItem = new vscode.TreeItem(
            `Budget: ${formatTokens(budget.used)} / ${formatTokens(budget.total)} (${budget.percent.toFixed(1)}%)`
        );
        budgetItem.iconPath = new vscode.ThemeIcon('dashboard');

        const nodes = this.cm.getContextNodes();
        const nodeItems = nodes.map(n => {
            const item = new vscode.TreeItem(n.label);
            item.description = `${n.kind} - ${formatTokens(n.tokens)}`;
            item.tooltip = n.path || n.id;
            item.iconPath = new vscode.ThemeIcon(kindToIcon(n.kind));
            return item;
        });

        return [budgetItem, ...nodeItems];
    }
}

class ActionsTreeProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData = new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

    getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
        return element;
    }

    getChildren(): vscode.TreeItem[] {
        const commands = [
            { label: 'Open Code Map', cmd: 'selfware.openCodeMap', icon: 'graph' },
            { label: 'Add to Context', cmd: 'selfware.contextAdd', icon: 'add' },
            { label: 'Remove from Context', cmd: 'selfware.contextRemove', icon: 'remove' },
            { label: 'Clear Context', cmd: 'selfware.contextClear', icon: 'clear-all' },
            { label: 'Inspect Component', cmd: 'selfware.inspect', icon: 'search' },
            { label: 'Build New Component', cmd: 'selfware.buildNew', icon: 'add' },
        ];
        return commands.map(c => {
            const item = new vscode.TreeItem(c.label);
            item.command = { command: c.cmd, title: c.label };
            item.iconPath = new vscode.ThemeIcon(c.icon);
            return item;
        });
    }
}

function formatTokens(t: number): string {
    if (t >= 1000) {
        return (t / 1000).toFixed(1) + 'k';
    }
    return '' + t;
}

function kindToIcon(kind: string): string {
    switch (kind) {
        case 'module': return 'package';
        case 'file': return 'file-code';
        case 'struct': return 'symbol-class';
        case 'function': return 'symbol-method';
        case 'trait': return 'symbol-interface';
        case 'impl': return 'symbol-class';
        case 'enum': return 'symbol-enum';
        default: return 'symbol-misc';
    }
}

export function deactivate() {
    // cleanup
}
