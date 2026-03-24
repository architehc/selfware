#!/usr/bin/env python3
"""
Pipeline Editor - Visual Workflow Builder for Selfware
Create, modify, and run agentic pipelines on-the-fly
"""

import asyncio
import json
import os
from pathlib import Path
from typing import Dict, List, Any, Optional
from datetime import datetime
import tempfile
import subprocess

# Pipeline storage
PIPELINES_DIR = Path("/tmp/selfware_pipelines")
PIPELINES_DIR.mkdir(exist_ok=True)

class PipelineNode:
    """A single node in the pipeline."""
    
    def __init__(self, node_id: str, node_type: str, config: Dict):
        self.id = node_id
        self.type = node_type  # 'build', 'validate', 'screenshot', 'vision', 'decide', 'improve'
        self.config = config
        self.inputs = []
        self.outputs = []
        self.status = 'pending'  # pending, running, completed, failed
        self.result = None
    
    def to_dict(self) -> Dict:
        return {
            'id': self.id,
            'type': self.type,
            'config': self.config,
            'inputs': self.inputs,
            'outputs': self.outputs,
            'status': self.status,
            'result': self.result
        }

class Pipeline:
    """A complete pipeline with nodes and connections."""
    
    def __init__(self, name: str, pipeline_id: Optional[str] = None):
        self.id = pipeline_id or f"pipeline_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
        self.name = name
        self.nodes: Dict[str, PipelineNode] = {}
        self.connections: List[tuple] = []  # (from_node, to_node)
        self.created_at = datetime.now().isoformat()
        self.status = 'draft'
    
    def add_node(self, node_type: str, config: Dict, node_id: Optional[str] = None) -> str:
        """Add a node to the pipeline."""
        node_id = node_id or f"{node_type}_{len(self.nodes)}"
        node = PipelineNode(node_id, node_type, config)
        self.nodes[node_id] = node
        return node_id
    
    def connect(self, from_node: str, to_node: str):
        """Connect two nodes."""
        if from_node in self.nodes and to_node in self.nodes:
            self.connections.append((from_node, to_node))
            self.nodes[to_node].inputs.append(from_node)
            self.nodes[from_node].outputs.append(to_node)
    
    def to_dict(self) -> Dict:
        return {
            'id': self.id,
            'name': self.name,
            'nodes': {k: v.to_dict() for k, v in self.nodes.items()},
            'connections': self.connections,
            'created_at': self.created_at,
            'status': self.status
        }
    
    def save(self):
        """Save pipeline to disk."""
        path = PIPELINES_DIR / f"{self.id}.json"
        with open(path, 'w') as f:
            json.dump(self.to_dict(), f, indent=2)
        return path
    
    @classmethod
    def load(cls, pipeline_id: str) -> 'Pipeline':
        """Load pipeline from disk."""
        path = PIPELINES_DIR / f"{pipeline_id}.json"
        with open(path, 'r') as f:
            data = json.load(f)
        
        pipeline = cls(data['name'], data['id'])
        pipeline.created_at = data['created_at']
        pipeline.status = data['status']
        
        # Reconstruct nodes
        for node_id, node_data in data['nodes'].items():
            node = PipelineNode(
                node_data['id'],
                node_data['type'],
                node_data['config']
            )
            node.inputs = node_data.get('inputs', [])
            node.outputs = node_data.get('outputs', [])
            pipeline.nodes[node_id] = node
        
        pipeline.connections = data['connections']
        return pipeline


class PipelineExecutor:
    """Execute pipelines with selfware integration."""
    
    def __init__(self, pipeline: Pipeline):
        self.pipeline = pipeline
        self.workspace = Path(f"/tmp/pipeline_exec_{pipeline.id}")
        self.workspace.mkdir(parents=True, exist_ok=True)
    
    async def execute_node(self, node: PipelineNode) -> Any:
        """Execute a single node."""
        
        print(f"\n[EXECUTING] {node.type} ({node.id})")
        node.status = 'running'
        
        try:
            if node.type == 'build':
                result = await self._execute_build(node)
            elif node.type == 'screenshot':
                result = await self._execute_screenshot(node)
            elif node.type == 'vision':
                result = await self._execute_vision(node)
            elif node.type == 'decide':
                result = await self._execute_decide(node)
            elif node.type == 'improve':
                result = await self._execute_improve(node)
            elif node.type == 'selfware':
                result = await self._execute_selfware(node)
            else:
                result = {'error': f'Unknown node type: {node.type}'}
            
            node.result = result
            node.status = 'completed' if not result.get('error') else 'failed'
            return result
            
        except Exception as e:
            node.status = 'failed'
            node.result = {'error': str(e)}
            return node.result
    
    async def _execute_build(self, node: PipelineNode) -> Dict:
        """Build website using selfware."""
        
        prompt = node.config.get('prompt', 'Create a simple website')
        output_dir = node.config.get('output_dir', str(self.workspace / 'website'))
        
        # Create selfware task
        task = f"""{prompt}

Create the website files in: {output_dir}
Include:
- index.html (main file)
- styles.css (if separate)
- script.js (if needed)
Make it beautiful, responsive, and professional."""
        
        # Run selfware
        proc = await asyncio.create_subprocess_exec(
            './target/release/selfware',
            '-c', 'selfware-stress-test.toml',
            'run', task,
            '-y',
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd='/home/ivo/selfware'
        )
        
        stdout, stderr = await proc.communicate()
        
        return {
            'output': stdout.decode(),
            'errors': stderr.decode(),
            'output_dir': output_dir,
            'files_created': list(Path(output_dir).glob('*')) if Path(output_dir).exists() else []
        }
    
    async def _execute_screenshot(self, node: PipelineNode) -> Dict:
        """Take screenshot of website."""
        
        url = node.config.get('url', 'http://localhost:8080')
        output_file = node.config.get('output', str(self.workspace / 'screenshot.png'))
        
        # Start server if needed
        website_dir = node.config.get('website_dir', str(self.workspace / 'website'))
        
        server = subprocess.Popen(
            ['python3', '-m', 'http.server', '8080'],
            cwd=website_dir,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
        
        await asyncio.sleep(2)  # Wait for server
        
        # Take screenshot using playwright
        screenshot_script = f"""
from playwright.sync_api import sync_playwright
with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={{'width': 1920, 'height': 1080}})
    page.goto('{url}')
    page.wait_for_load_state('networkidle')
    page.screenshot(path='{output_file}', full_page=True)
    browser.close()
    print('Screenshot saved to {output_file}')
"""
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
            f.write(screenshot_script)
            script_path = f.name
        
        result = subprocess.run(
            ['python3', script_path],
            capture_output=True,
            text=True,
            timeout=30
        )
        
        os.unlink(script_path)
        server.terminate()
        
        return {
            'screenshot_path': output_file,
            'success': os.path.exists(output_file),
            'stdout': result.stdout,
            'stderr': result.stderr
        }
    
    async def _execute_vision(self, node: PipelineNode) -> Dict:
        """Analyze screenshot with vision model."""
        
        screenshot_path = node.config.get('screenshot_path', '')
        prompt = node.config.get('prompt', 'Analyze this website design')
        
        # For now, return a placeholder - actual vision call would be here
        return {
            'analysis': 'Vision analysis placeholder',
            'screenshot': screenshot_path,
            'issues_found': [],
            'suggestions': []
        }
    
    async def _execute_decide(self, node: PipelineNode) -> Dict:
        """Decision node - should we continue or stop?"""
        
        condition = node.config.get('condition', 'always_continue')
        max_iterations = node.config.get('max_iterations', 5)
        current_iteration = node.config.get('current_iteration', 0)
        
        if current_iteration >= max_iterations:
            should_continue = False
            reason = 'Max iterations reached'
        elif condition == 'no_issues':
            # Check previous vision results
            should_continue = False
            reason = 'No critical issues found'
        else:
            should_continue = True
            reason = 'Continuing as configured'
        
        return {
            'should_continue': should_continue,
            'reason': reason,
            'next_action': 'improve' if should_continue else 'finish'
        }
    
    async def _execute_improve(self, node: PipelineNode) -> Dict:
        """Improve website based on feedback."""
        
        feedback = node.config.get('feedback', '')
        website_dir = node.config.get('website_dir', str(self.workspace / 'website'))
        
        task = f"""Improve the website based on this feedback:

FEEDBACK:
{feedback}

Website location: {website_dir}
Make the improvements and update the files."""
        
        proc = await asyncio.create_subprocess_exec(
            './target/release/selfware',
            '-c', 'selfware-stress-test.toml',
            'run', task,
            '-y',
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd='/home/ivo/selfware'
        )
        
        stdout, stderr = await proc.communicate()
        
        return {
            'improvements_made': True,
            'output': stdout.decode(),
            'errors': stderr.decode()
        }
    
    async def _execute_selfware(self, node: PipelineNode) -> Dict:
        """Execute arbitrary selfware command."""
        
        task = node.config.get('task', 'Help with something')
        
        proc = await asyncio.create_subprocess_exec(
            './target/release/selfware',
            '-c', 'selfware-stress-test.toml',
            'run', task,
            '-y',
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd='/home/ivo/selfware'
        )
        
        stdout, stderr = await proc.communicate()
        
        return {
            'output': stdout.decode(),
            'errors': stderr.decode()
        }
    
    async def execute(self) -> Dict:
        """Execute the full pipeline."""
        
        print(f"\n{'='*60}")
        print(f"Executing Pipeline: {self.pipeline.name}")
        print(f"{'='*60}\n")
        
        self.pipeline.status = 'running'
        results = {}
        
        # Topological sort for execution order
        executed = set()
        pending = set(self.pipeline.nodes.keys())
        
        while pending:
            # Find nodes with all inputs executed
            ready = []
            for node_id in pending:
                node = self.pipeline.nodes[node_id]
                if all(inp in executed for inp in node.inputs):
                    ready.append(node_id)
            
            if not ready:
                raise ValueError("Circular dependency detected")
            
            # Execute ready nodes
            for node_id in ready:
                node = self.pipeline.nodes[node_id]
                result = await self.execute_node(node)
                results[node_id] = result
                executed.add(node_id)
                pending.remove(node_id)
        
        self.pipeline.status = 'completed'
        return results


def create_website_validation_pipeline(name: str, design_prompt: str) -> Pipeline:
    """Create a pre-configured website validation pipeline."""
    
    pipeline = Pipeline(name)
    
    # Node 1: Build
    build_node = pipeline.add_node('build', {
        'prompt': design_prompt,
        'output_dir': f'/tmp/pipeline_exec_{pipeline.id}/website'
    })
    
    # Node 2: Screenshot
    screenshot_node = pipeline.add_node('screenshot', {
        'url': 'http://localhost:8080',
        'website_dir': f'/tmp/pipeline_exec_{pipeline.id}/website',
        'output': f'/tmp/pipeline_exec_{pipeline.id}/screenshot_v1.png'
    })
    
    # Node 3: Vision Analysis
    vision_node = pipeline.add_node('vision', {
        'screenshot_path': f'/tmp/pipeline_exec_{pipeline.id}/screenshot_v1.png',
        'prompt': 'Analyze this website design and identify issues'
    })
    
    # Node 4: Decision
    decide_node = pipeline.add_node('decide', {
        'condition': 'has_issues',
        'max_iterations': 3,
        'current_iteration': 0
    })
    
    # Node 5: Improve (if needed)
    improve_node = pipeline.add_node('improve', {
        'feedback': '{{vision.analysis}}',
        'website_dir': f'/tmp/pipeline_exec_{pipeline.id}/website'
    })
    
    # Connect nodes
    pipeline.connect(build_node, screenshot_node)
    pipeline.connect(screenshot_node, vision_node)
    pipeline.connect(vision_node, decide_node)
    pipeline.connect(decide_node, improve_node)
    
    return pipeline


def interactive_pipeline_editor():
    """Interactive CLI for building pipelines."""
    
    print("╔════════════════════════════════════════════════════════════╗")
    print("║           PIPELINE EDITOR - Selfware Workflows            ║")
    print("╚════════════════════════════════════════════════════════════╝\n")
    
    print("Commands:")
    print("  create <name>     - Create new pipeline")
    print("  load <id>         - Load existing pipeline")
    print("  add <type>        - Add node (build|screenshot|vision|decide|improve|selfware)")
    print("  connect <a> <b>   - Connect nodes")
    print("  list              - List all nodes")
    print("  save              - Save pipeline")
    print("  run               - Execute pipeline")
    print("  quit              - Exit\n")
    
    current_pipeline = None
    
    while True:
        try:
            cmd = input("pipeline> ").strip().split()
            if not cmd:
                continue
            
            if cmd[0] == 'create':
                name = ' '.join(cmd[1:]) if len(cmd) > 1 else 'untitled'
                current_pipeline = Pipeline(name)
                print(f"Created pipeline: {name} ({current_pipeline.id})")
            
            elif cmd[0] == 'load':
                if len(cmd) < 2:
                    print("Usage: load <pipeline_id>")
                    continue
                current_pipeline = Pipeline.load(cmd[1])
                print(f"Loaded: {current_pipeline.name}")
            
            elif cmd[0] == 'add':
                if not current_pipeline:
                    print("Create a pipeline first!")
                    continue
                if len(cmd) < 2:
                    print("Usage: add <node_type>")
                    continue
                
                node_type = cmd[1]
                config = {}
                
                if node_type == 'build':
                    config['prompt'] = input("Build prompt: ")
                    config['output_dir'] = input("Output directory [/tmp/website]: ") or '/tmp/website'
                elif node_type == 'screenshot':
                    config['url'] = input("URL [http://localhost:8080]: ") or 'http://localhost:8080'
                elif node_type == 'selfware':
                    config['task'] = input("Selfware task: ")
                
                node_id = current_pipeline.add_node(node_type, config)
                print(f"Added node: {node_id}")
            
            elif cmd[0] == 'connect':
                if not current_pipeline or len(cmd) < 3:
                    print("Usage: connect <from_node> <to_node>")
                    continue
                current_pipeline.connect(cmd[1], cmd[2])
                print(f"Connected: {cmd[1]} -> {cmd[2]}")
            
            elif cmd[0] == 'list':
                if not current_pipeline:
                    print("No pipeline loaded")
                    continue
                print(f"\nPipeline: {current_pipeline.name}")
                print(f"Nodes: {len(current_pipeline.nodes)}")
                for node_id, node in current_pipeline.nodes.items():
                    print(f"  [{node.type}] {node_id}")
                    if node.outputs:
                        print(f"    -> {', '.join(node.outputs)}")
            
            elif cmd[0] == 'save':
                if not current_pipeline:
                    print("No pipeline to save")
                    continue
                path = current_pipeline.save()
                print(f"Saved to: {path}")
            
            elif cmd[0] == 'run':
                if not current_pipeline:
                    print("No pipeline to run")
                    continue
                executor = PipelineExecutor(current_pipeline)
                asyncio.run(executor.execute())
            
            elif cmd[0] == 'quit':
                break
            
            else:
                print(f"Unknown command: {cmd[0]}")
        
        except KeyboardInterrupt:
            print("\nUse 'quit' to exit")
        except Exception as e:
            print(f"Error: {e}")


if __name__ == "__main__":
    import sys
    
    if len(sys.argv) > 1 and sys.argv[1] == '--interactive':
        interactive_pipeline_editor()
    else:
        # Quick test - create and run a website validation pipeline
        design = """Create a modern landing page for an AI development tool.
Features:
- Hero section with gradient background
- Feature cards with icons
- Pricing table
- Contact CTA
Dark theme with purple/blue accents."""
        
        pipeline = create_website_validation_pipeline("AI Tool Landing Page", design)
        pipeline.save()
        
        print(f"Created pipeline: {pipeline.id}")
        print(f"Run with: python3 pipeline_editor.py --interactive")
        print(f"Then: load {pipeline.id}")
        print(f"Then: run")
