#!/usr/bin/env python3
"""
Comprehensive load testing suite for Mote infrastructure
Simulates 100+ concurrent agents with various operations
"""
import asyncio
import aiohttp
import json
import time
import random
import statistics
from datetime import datetime, timedelta
from cryptography.hazmat.primitives.asymmetric import ed25519
import binascii
from concurrent.futures import ThreadPoolExecutor
import argparse
import logging

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

class LoadTestMetrics:
    def __init__(self):
        self.reset()
    
    def reset(self):
        self.agent_registrations = []
        self.agent_confirmations = []
        self.bounty_publications = []
        self.finding_publications = []
        self.search_requests = []
        self.suggestion_requests = []
        self.claim_requests = []
        self.errors = []
        self.start_time = None
        self.end_time = None
    
    def record_latency(self, operation, latency, success=True):
        timestamp = time.time()
        record = {
            'timestamp': timestamp,
            'latency': latency,
            'success': success
        }
        
        if operation == 'register':
            self.agent_registrations.append(record)
        elif operation == 'confirm':
            self.agent_confirmations.append(record)
        elif operation == 'publish_bounty':
            self.bounty_publications.append(record)
        elif operation == 'publish_finding':
            self.finding_publications.append(record)
        elif operation == 'search':
            self.search_requests.append(record)
        elif operation == 'suggestions':
            self.suggestion_requests.append(record)
        elif operation == 'claim':
            self.claim_requests.append(record)
        elif not success:
            self.errors.append({
                'timestamp': timestamp,
                'operation': operation,
                'latency': latency,
                'error': 'Operation failed'
            })
    
    def get_stats(self, operation_records):
        if not operation_records:
            return {'count': 0, 'avg_latency': 0, 'min_latency': 0, 'max_latency': 0, 'p95': 0, 'p99': 0}
        
        latencies = [r['latency'] for r in operation_records if r['success']]
        if not latencies:
            return {'count': len(operation_records), 'avg_latency': 0, 'min_latency': 0, 'max_latency': 0, 'p95': 0, 'p99': 0}
        
        return {
            'count': len(operation_records),
            'avg_latency': statistics.mean(latencies),
            'min_latency': min(latencies),
            'max_latency': max(latencies),
            'p95': statistics.quantiles(latencies, n=20)[18] if len(latencies) > 20 else max(latencies),
            'p99': statistics.quantiles(latencies, n=100)[98] if len(latencies) > 100 else max(latencies)
        }
    
    def print_summary(self):
        duration = (self.end_time - self.start_time) if self.start_time and self.end_time else 0
        
        print(f"\n{'='*80}")
        print(f"LOAD TEST SUMMARY - Duration: {duration:.2f}s")
        print(f"{'='*80}")
        
        # Agent operations
        reg_stats = self.get_stats(self.agent_registrations)
        conf_stats = self.get_stats(self.agent_confirmations)
        print(f"🔐 Agent Registrations: {reg_stats['count']} | Avg: {reg_stats['avg_latency']:.3f}s | P95: {reg_stats['p95']:.3f}s")
        print(f"🔑 Agent Confirmations: {conf_stats['count']} | Avg: {conf_stats['avg_latency']:.3f}s | P95: {conf_stats['p95']:.3f}s")
        
        # Publishing operations
        bounty_stats = self.get_stats(self.bounty_publications)
        finding_stats = self.get_stats(self.finding_publications)
        print(f"🎯 Bounty Publications: {bounty_stats['count']} | Avg: {bounty_stats['avg_latency']:.3f}s | P95: {bounty_stats['p95']:.3f}s")
        print(f"🔬 Finding Publications: {finding_stats['count']} | Avg: {finding_stats['avg_latency']:.3f}s | P95: {finding_stats['p95']:.3f}s")
        
        # Query operations
        search_stats = self.get_stats(self.search_requests)
        sugg_stats = self.get_stats(self.suggestion_requests)
        claim_stats = self.get_stats(self.claim_requests)
        print(f"🔍 Search Requests: {search_stats['count']} | Avg: {search_stats['avg_latency']:.3f}s | P95: {search_stats['p95']:.3f}s")
        print(f"💡 Suggestion Requests: {sugg_stats['count']} | Avg: {sugg_stats['avg_latency']:.3f}s | P95: {sugg_stats['p95']:.3f}s")
        print(f"🏁 Claim Requests: {claim_stats['count']} | Avg: {claim_stats['avg_latency']:.3f}s | P95: {claim_stats['p95']:.3f}s")
        
        # Errors
        print(f"❌ Total Errors: {len(self.errors)}")
        if self.errors:
            error_rate = len(self.errors) / (reg_stats['count'] + conf_stats['count'] + bounty_stats['count'] + finding_stats['count'] + search_stats['count'] + sugg_stats['count'] + claim_stats['count']) * 100
            print(f"📊 Error Rate: {error_rate:.2f}%")
        
        # Throughput
        total_ops = reg_stats['count'] + conf_stats['count'] + bounty_stats['count'] + finding_stats['count'] + search_stats['count'] + sugg_stats['count'] + claim_stats['count']
        throughput = total_ops / duration if duration > 0 else 0
        print(f"🚀 Total Operations: {total_ops} | Throughput: {throughput:.2f} ops/sec")
        print(f"{'='*80}")

class MoteLoadTestAgent:
    def __init__(self, agent_id, base_url="http://localhost:3000"):
        self.agent_id = agent_id
        self.base_url = base_url
        self.rpc_url = f"{base_url}/rpc"  # Use RPC endpoint instead of MCP
        self.private_key = ed25519.Ed25519PrivateKey.generate()
        self.public_key = self.private_key.public_key()
        self.registered_agent_id = None
        self.session_id = None
        self.metrics = LoadTestMetrics()
        
    async def make_rpc_request(self, session, method, params=None, request_id=1):
        """Make a JSON-RPC 2.0 request to Mote RPC endpoint"""
        start_time = time.time()
        try:
            payload = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params or {},
                "id": request_id
            }
            
            async with session.post(self.rpc_url, json=payload, timeout=aiohttp.ClientTimeout(total=30)) as response:
                result = await response.json()
                latency = time.time() - start_time
                
                if response.status == 200 and 'result' in result:
                    return True, latency, result
                else:
                    return False, latency, result
        except Exception as e:
            latency = time.time() - start_time
            logger.error(f"Request failed for {method}: {e}")
            return False, latency, {"error": str(e)}
    
    async def register(self, session):
        """Register the agent with Mote"""
        success, latency, response = await self.make_rpc_request(
            session, "register_agent", {
                "public_key": binascii.hexlify(self.public_key.public_bytes_raw()).decode()
            }, self.agent_id * 1000 + 1
        )
        
        if success and 'result' in response:
            self.registered_agent_id = response['result']['agent_id']
            self.challenge = response['result']['challenge']
        
        self.metrics.record_latency('register', latency, success)
        return success
    
    async def confirm(self, session):
        """Confirm agent registration by signing challenge"""
        if not self.registered_agent_id:
            return False
        
        challenge_bytes = binascii.unhexlify(self.challenge)
        signature = self.private_key.sign(challenge_bytes)
        signature_hex = binascii.hexlify(signature).decode()
        
        success, latency, response = await self.make_rpc_request(
            session, "confirm_agent", {
                "agent_id": self.registered_agent_id,
                "signature": signature_hex
            }, self.agent_id * 1000 + 2
        )
        
        self.metrics.record_latency('confirm', latency, success)
        return success
    
    async def publish_bounty(self, session):
        """Publish a bounty atom"""
        if not self.registered_agent_id:
            return False
        
        # Create atom signature (mock for testing)
        atom_signature = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        atom_signature_bytes = binascii.unhexlify(atom_signature)
        
        # Prepare the atom data
        atom_data = {
            "atom_type": "bounty",
            "domain": random.choice(["machine_learning", "nlp", "computer_vision", "robotics", "bioinformatics"]),
            "statement": f"Improve accuracy of {random.choice(['image classification', 'text generation', 'object detection', 'path planning', 'protein folding'])} models on {random.choice(['medical', 'financial', 'autonomous', 'industrial', 'research'])} datasets",
            "conditions": {
                "task": random.choice(["classification", "generation", "detection", "planning", "prediction"]),
                "dataset_type": random.choice(["medical", "financial", "autonomous", "industrial", "research"]),
                "target_metric": random.choice(["accuracy", "f1_score", "precision", "recall", "mse"]),
                "min_improvement": round(random.uniform(0.01, 0.15), 3)
            },
            "metrics": None,
            "provenance": {
                "parent_ids": [],
                "code_hash": None,
                "environment": None,
                "dataset_fingerprint": None,
                "experiment_ref": None,
                "method_description": None
            },
            "signature": list(atom_signature_bytes)
        }
        
        # Prepare request with atom signature
        request_data = {
            "agent_id": self.registered_agent_id,
            "atoms": [atom_data]
        }
        
        # Generate top-level signature
        canonical_json = json.dumps(request_data, separators=(',', ':'), sort_keys=True)
        top_signature = self.private_key.sign(canonical_json.encode())
        top_signature_hex = binascii.hexlify(top_signature).decode()
        
        # Final request
        final_request = {
            "agent_id": self.registered_agent_id,
            "signature": top_signature_hex,
            "atoms": [atom_data]
        }
        
        success, latency, response = await self.make_rpc_request(
            session, "publish_atoms", final_request, self.agent_id * 1000 + 3
        )
        
        self.metrics.record_latency('publish_bounty', latency, success)
        return success
    
    async def publish_finding(self, session):
        """Publish a finding atom"""
        if not self.registered_agent_id:
            return False
        
        # Create atom signature (mock for testing)
        atom_signature = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
        atom_signature_bytes = binascii.unhexlify(atom_signature)
        
        # Prepare the atom data
        atom_data = {
            "atom_type": "finding",
            "domain": random.choice(["machine_learning", "nlp", "computer_vision", "robotics", "bioinformatics"]),
            "statement": f"Achieved {random.uniform(0.85, 0.99):.3f} accuracy on {random.choice(['image classification', 'text generation', 'object detection', 'path planning', 'protein folding'])} using {random.choice(['transformer', 'cnn', 'rnn', 'gnn', 'hybrid'])} architecture",
            "conditions": {
                "model_type": random.choice(["transformer", "cnn", "rnn", "gnn", "hybrid"]),
                "dataset": random.choice(["imagenet", "coco", "squad", "kinetics", "pdb"]),
                "training_epochs": random.randint(10, 100),
                "batch_size": random.choice([16, 32, 64, 128, 256])
            },
            "metrics": {
                "accuracy": round(random.uniform(0.85, 0.99), 4),
                "f1_score": round(random.uniform(0.83, 0.97), 4),
                "precision": round(random.uniform(0.86, 0.98), 4),
                "recall": round(random.uniform(0.82, 0.96), 4),
                "training_time_hours": round(random.uniform(1.0, 24.0), 2)
            },
            "provenance": {
                "parent_ids": [],
                "code_hash": None,
                "environment": None,
                "dataset_fingerprint": None,
                "experiment_ref": None,
                "method_description": None
            },
            "signature": list(atom_signature_bytes)
        }
        
        # Prepare request with atom signature
        request_data = {
            "agent_id": self.registered_agent_id,
            "atoms": [atom_data]
        }
        
        # Generate top-level signature
        canonical_json = json.dumps(request_data, separators=(',', ':'), sort_keys=True)
        top_signature = self.private_key.sign(canonical_json.encode())
        top_signature_hex = binascii.hexlify(top_signature).decode()
        
        # Final request
        final_request = {
            "agent_id": self.registered_agent_id,
            "signature": top_signature_hex,
            "atoms": [atom_data]
        }
        
        success, latency, response = await self.make_rpc_request(
            session, "publish_atoms", final_request, self.agent_id * 1000 + 4
        )
        
        self.metrics.record_latency('publish_finding', latency, success)
        return success
    
    async def search(self, session):
        """Search for atoms"""
        success, latency, response = await self.make_rpc_request(
            session, "search_atoms", {
                "domain": random.choice(["machine_learning", "nlp", "computer_vision"]),
                "atom_types": random.sample(["bounty", "finding"], k=random.randint(1, 2))
            }, self.agent_id * 1000 + 5
        )
        
        self.metrics.record_latency('search', latency, success)
        return success
    
    async def get_suggestions(self, session):
        """Get suggestions from Mote"""
        success, latency, response = await self.make_rpc_request(
            session, "get_suggestions", {
                "context": {"domain": random.choice(["machine_learning", "nlp", "computer_vision"])},
                "k": random.randint(5, 20)
            }, self.agent_id * 1000 + 6
        )
        
        self.metrics.record_latency('suggestions', latency, success)
        return success
    
    async def claim_direction(self, session):
        """Claim a direction for work"""
        if not self.registered_agent_id:
            return False
        
        success, latency, response = await self.make_rpc_request(
            session, "claim_direction", {
                "agent_id": self.registered_agent_id,
                "context": {"domain": random.choice(["machine_learning", "nlp", "computer_vision"])},
                "max_claims": random.randint(1, 5)
            }, self.agent_id * 1000 + 7
        )
        
        self.metrics.record_latency('claim', latency, success)
        return success

async def run_agent_workload(agent, session, operations_per_agent, think_time_range=(0.1, 0.5)):
    """Run a mixed workload for a single agent"""
    # Registration phase
    await agent.register(session)
    await asyncio.sleep(random.uniform(*think_time_range))
    
    await agent.confirm(session)
    await asyncio.sleep(random.uniform(*think_time_range))
    
    # Mixed workload phase
    for i in range(operations_per_agent):
        operation = random.choice([
            ('publish_bounty', 0.3),
            ('publish_finding', 0.4),
            ('search', 0.15),
            ('get_suggestions', 0.1),
            ('claim_direction', 0.05)
        ])
        
        method = operation[0]
        
        if method == 'publish_bounty':
            await agent.publish_bounty(session)
        elif method == 'publish_finding':
            await agent.publish_finding(session)
        elif method == 'search':
            await agent.search(session)
        elif method == 'get_suggestions':
            await agent.get_suggestions(session)
        elif method == 'claim_direction':
            await agent.claim_direction(session)
        
        # Random think time between operations
        await asyncio.sleep(random.uniform(*think_time_range))

async def run_load_test(num_agents=100, operations_per_agent=10, concurrent_batches=5):
    """Run the full load test"""
    logger.info(f"Starting load test: {num_agents} agents, {operations_per_agent} ops each")
    
    global_metrics = LoadTestMetrics()
    global_metrics.start_time = time.time()
    
    # Create agents
    agents = [MoteLoadTestAgent(i) for i in range(num_agents)]
    
    # Configure connector limits for high concurrency
    connector = aiohttp.TCPConnector(
        limit=200,  # Total connection pool size
        limit_per_host=50,  # Connections per host
        ttl_dns_cache=300,
        use_dns_cache=True,
    )
    
    timeout = aiohttp.ClientTimeout(total=60, connect=10)
    
    async with aiohttp.ClientSession(connector=connector, timeout=timeout) as session:
        # Run agents in batches to avoid overwhelming the system
        batch_size = num_agents // concurrent_batches
        tasks = []
        
        for batch in range(concurrent_batches):
            start_idx = batch * batch_size
            end_idx = start_idx + batch_size if batch < concurrent_batches - 1 else num_agents
            
            logger.info(f"Starting batch {batch + 1}/{concurrent_batches} with agents {start_idx}-{end_idx-1}")
            
            batch_tasks = [
                run_agent_workload(agents[i], session, operations_per_agent)
                for i in range(start_idx, end_idx)
            ]
            
            tasks.extend(batch_tasks)
            
            # Start current batch
            if batch < concurrent_batches - 1:
                # Wait a bit before starting next batch
                await asyncio.sleep(2.0)
        
        # Execute all tasks
        logger.info(f"Executing {len(tasks)} concurrent agent workloads")
        await asyncio.gather(*tasks, return_exceptions=True)
    
    global_metrics.end_time = time.time()
    
    # Aggregate metrics from all agents
    for agent in agents:
        global_metrics.agent_registrations.extend(agent.metrics.agent_registrations)
        global_metrics.agent_confirmations.extend(agent.metrics.agent_confirmations)
        global_metrics.bounty_publications.extend(agent.metrics.bounty_publications)
        global_metrics.finding_publications.extend(agent.metrics.finding_publications)
        global_metrics.search_requests.extend(agent.metrics.search_requests)
        global_metrics.suggestion_requests.extend(agent.metrics.suggestion_requests)
        global_metrics.claim_requests.extend(agent.metrics.claim_requests)
        global_metrics.errors.extend(agent.metrics.errors)
    
    global_metrics.print_summary()
    return global_metrics

async def main():
    parser = argparse.ArgumentParser(description='Mote Load Testing Suite')
    parser.add_argument('--agents', type=int, default=100, help='Number of concurrent agents')
    parser.add_argument('--operations', type=int, default=10, help='Operations per agent')
    parser.add_argument('--batches', type=int, default=5, help='Number of concurrent batches')
    parser.add_argument('--url', default='http://localhost:3000', help='Mote server URL')
    
    args = parser.parse_args()
    
    print(f"🚀 Mote Load Testing Suite")
    print(f"📊 Configuration: {args.agents} agents, {args.operations} ops/agent, {args.batches} batches")
    print(f"🌐 Target URL: {args.url}")
    print(f"⏰ Started at: {datetime.now().isoformat()}")
    
    try:
        metrics = await run_load_test(args.agents, args.operations, args.batches)
        
        print(f"\n✅ Load test completed successfully!")
        print(f"📈 Check server metrics at: {args.url}/metrics")
        
    except Exception as e:
        logger.error(f"Load test failed: {e}")
        print(f"❌ Load test failed: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit_code = asyncio.run(main())
    exit(exit_code)
