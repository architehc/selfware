export interface GraphNode {
    id: string;
    label: string;
    kind: string;           // module, file, struct, function, trait, impl, enum
    tokens: number;
    path?: string;
    fusion_group?: string;  // binary, trinary, quaternary
    children?: string[];
    deps?: string[];
}

export interface GraphEdge {
    source: string;
    target: string;
    kind: string;           // contains, depends, implements, calls
}

export interface CodeGraph {
    nodes: GraphNode[];
    edges: GraphEdge[];
}

export type ActionType =
    | 'inspect'
    | 'read_full'
    | 'read_skeleton'
    | 'alter'
    | 'build_new'
    | 'verify'
    | 'test'
    | 'ship'
    | 'git_diff';

export interface Action {
    type: ActionType;
    label: string;
    tokenCost: number;
    description: string;
}

export class ContextManager {
    private contextNodes: Map<string, GraphNode> = new Map();
    private totalBudget: number;
    private graph: CodeGraph | null = null;
    private onChangeCallbacks: Array<() => void> = [];

    constructor(totalBudget: number = 1_000_000) {
        this.totalBudget = totalBudget;
    }

    setGraph(graph: CodeGraph): void {
        this.graph = graph;
    }

    getGraph(): CodeGraph | null {
        return this.graph;
    }

    findNode(nodeId: string): GraphNode | undefined {
        return this.graph?.nodes.find(n => n.id === nodeId);
    }

    addToContext(nodeId: string): boolean {
        if (this.contextNodes.has(nodeId)) {
            return false;
        }
        const node = this.findNode(nodeId);
        if (!node) {
            return false;
        }
        const used = this.getUsedTokens();
        if (used + node.tokens > this.totalBudget) {
            return false;
        }
        this.contextNodes.set(nodeId, node);
        this.notifyChange();
        return true;
    }

    removeFromContext(nodeId: string): boolean {
        const removed = this.contextNodes.delete(nodeId);
        if (removed) {
            this.notifyChange();
        }
        return removed;
    }

    clearContext(): void {
        this.contextNodes.clear();
        this.notifyChange();
    }

    isInContext(nodeId: string): boolean {
        return this.contextNodes.has(nodeId);
    }

    getUsedTokens(): number {
        let total = 0;
        for (const node of this.contextNodes.values()) {
            total += node.tokens;
        }
        return total;
    }

    getTokenBudget(): { used: number; total: number; percent: number } {
        const used = this.getUsedTokens();
        return {
            used,
            total: this.totalBudget,
            percent: this.totalBudget > 0 ? (used / this.totalBudget) * 100 : 0,
        };
    }

    getContextNodes(): GraphNode[] {
        return Array.from(this.contextNodes.values());
    }

    getContextNodeIds(): string[] {
        return Array.from(this.contextNodes.keys());
    }

    suggestActions(nodeId: string): Action[] {
        const node = this.findNode(nodeId);
        if (!node) {
            return [];
        }

        const t = node.tokens;
        const actions: Action[] = [
            {
                type: 'inspect',
                label: 'Inspect',
                tokenCost: Math.ceil(t * 0.1),
                description: 'View summary: signature, deps, doc comment',
            },
            {
                type: 'read_skeleton',
                label: 'Read Skeleton',
                tokenCost: Math.ceil(t * 0.3),
                description: 'Type signatures and structure without bodies',
            },
            {
                type: 'read_full',
                label: 'Read Full',
                tokenCost: t,
                description: 'Full source code',
            },
            {
                type: 'alter',
                label: 'Alter',
                tokenCost: Math.ceil(t * 1.5),
                description: 'Modify this component (read + write + verify)',
            },
            {
                type: 'build_new',
                label: 'Build New',
                tokenCost: Math.ceil(t * 0.5),
                description: 'Create a new sibling component',
            },
            {
                type: 'verify',
                label: 'Verify',
                tokenCost: Math.ceil(t * 0.2),
                description: 'Run clippy/check on this module',
            },
            {
                type: 'test',
                label: 'Test',
                tokenCost: Math.ceil(t * 0.4),
                description: 'Run tests for this component',
            },
            {
                type: 'ship',
                label: 'Ship',
                tokenCost: Math.ceil(t * 0.1),
                description: 'Stage, commit, push changes',
            },
            {
                type: 'git_diff',
                label: 'Git Diff',
                tokenCost: Math.ceil(t * 0.2),
                description: 'Show uncommitted changes',
            },
        ];

        return actions;
    }

    onChange(callback: () => void): void {
        this.onChangeCallbacks.push(callback);
    }

    private notifyChange(): void {
        for (const cb of this.onChangeCallbacks) {
            cb();
        }
    }
}
