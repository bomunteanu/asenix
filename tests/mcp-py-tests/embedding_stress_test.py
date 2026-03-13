#!/usr/bin/env python3
"""
Embedding Queue Stress Test
Specifically tests the ≤32 worker pool bounds and pgvector HNSW contention
"""
import asyncio
import aiohttp
import json
import time
import random
import statistics
from datetime import datetime
from cryptography.hazmat.primitives.asymmetric import ed25519
import binascii
import logging
import argparse

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

class EmbeddingStressTest:
    def __init__(self, base_url="http://localhost:3000"):
        self.base_url = base_url
        self.mcp_url = f"{base_url}/mcp"
        self.metrics_url = f"{base_url}/metrics"
        self.agents = []
        self.published_atoms = []
        
    async def create_agent(self, session, agent_id):
        """Create and register an agent with proper session management"""
        private_key = ed25519.Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        
        # Initialize MCP session
        init_payload = {
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocol_version": "2025-03-26",
                "capabilities": {
                    "tools": {},
                    "resources": {}
                },
                "client_info": {
                    "name": f"embedding-test-agent-{agent_id}",
                    "version": "1.0.0"
                }
            },
            "id": agent_id * 1000
        }
        
        headers = {
            "content-type": "application/json",
            "origin": "http://localhost:3000",
            "accept": "application/json, text/event-stream"
        }
        
        async with session.post(self.mcp_url, json=init_payload, headers=headers) as response:
            result = await response.json()
            if 'result' not in result:
                return None
            
            session_id = result['result'].get('session_id')
        
        # Send initialized notification
        notify_payload = {
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
            "id": agent_id * 1000 + 1
        }
        
        notify_headers = headers.copy()
        notify_headers["mcp-session-id"] = session_id
        
        async with session.post(self.mcp_url, json=notify_payload, headers=notify_headers) as response:
            await response.json()  # We don't care about the response
        
        # Register agent
        reg_payload = {
            "jsonrpc": "2.0",
            "method": "register_agent",
            "params": {
                "public_key": binascii.hexlify(public_key.public_bytes_raw()).decode()
            },
            "id": agent_id * 1000 + 2
        }
        
        reg_headers = headers.copy()
        reg_headers["mcp-session-id"] = session_id
        
        async with session.post(self.mcp_url, json=reg_payload, headers=reg_headers) as response:
            result = await response.json()
            if 'result' in result:
                agent_info = {
                    'agent_id': result['result']['agent_id'],
                    'challenge': result['result']['challenge'],
                    'private_key': private_key,
                    'session_id': session_id
                }
                
                # Confirm agent
                challenge_bytes = binascii.unhexlify(result['result']['challenge'])
                signature = private_key.sign(challenge_bytes)
                signature_hex = binascii.hexlify(signature).decode()
                
                conf_payload = {
                    "jsonrpc": "2.0",
                    "method": "confirm_agent",
                    "params": {
                        "agent_id": agent_info['agent_id'],
                        "signature": signature_hex
                    },
                    "id": agent_id * 1000 + 3
                }
                
                conf_headers = headers.copy()
                conf_headers["mcp-session-id"] = session_id
                
                async with session.post(self.mcp_url, json=conf_payload, headers=conf_headers) as response:
                    conf_result = await response.json()
                    if 'result' in conf_result:
                        return agent_info
                
                return agent_info
            else:
                return None
    
    async def publish_atoms_batch(self, session, agent, batch_size=10):
        """Publish a batch of atoms to stress the embedding queue"""
        atoms = []
        
        for i in range(batch_size):
            # Create atom signature (mock for testing)
            atom_signature = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
            atom_signature_bytes = binascii.unhexlify(atom_signature)
            
            atom_data = {
                "atom_type": random.choice(["finding", "hypothesis", "negative_result"]),
                "domain": random.choice(["machine_learning", "nlp", "computer_vision", "robotics"]),
                "statement": f"Research result {i}: {random.choice(['Improved model performance', 'New algorithm developed', 'Dataset analysis complete', 'Theory validation successful', 'Experimental results obtained'])} with {random.uniform(0.01, 0.99):.3f} confidence",
                "conditions": {
                    "experiment_type": random.choice(["controlled", "observational", "simulation", "theoretical"]),
                    "sample_size": random.randint(100, 10000),
                    "reproducible": random.choice([True, False])
                },
                "metrics": {
                    "accuracy": round(random.uniform(0.7, 0.95), 4),
                    "precision": round(random.uniform(0.65, 0.93), 4),
                    "recall": round(random.uniform(0.68, 0.91), 4),
                    "f1_score": round(random.uniform(0.67, 0.92), 4),
                    "p_value": round(random.uniform(0.001, 0.05), 4),
                    "effect_size": round(random.uniform(0.2, 0.8), 3)
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
            atoms.append(atom_data)
        
        # Prepare request with atom signatures
        request_data = {
            "agent_id": agent['agent_id'],
            "atoms": atoms
        }
        
        # Generate top-level signature
        canonical_json = json.dumps(request_data, separators=(',', ':'), sort_keys=True)
        top_signature = agent['private_key'].sign(canonical_json.encode())
        top_signature_hex = binascii.hexlify(top_signature).decode()
        
        # Final request
        final_request = {
            "agent_id": agent['agent_id'],
            "signature": top_signature_hex,
            "atoms": atoms
        }
        
        payload = {
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": final_request,
            "id": int(time.time() * 1000) % 1000000
        }
        
        headers = {
            "content-type": "application/json",
            "origin": "http://localhost:3000",
            "accept": "application/json, text/event-stream"
        }
        
        if agent.get('session_id'):
            headers["mcp-session-id"] = agent['session_id']
        
        start_time = time.time()
        async with session.post(self.mcp_url, json=payload, headers=headers) as response:
            result = await response.json()
            latency = time.time() - start_time
            
            if response.status == 200 and 'result' in result:
                atom_ids = result['result'].get('published_atoms', [])  # Fixed: was 'atom_ids'
                self.published_atoms.extend(atom_ids)
                return True, latency, len(atom_ids)
            else:
                print(f"Publish failed: {result}")
                return False, latency, 0
    
    async def get_metrics(self, session):
        """Fetch current server metrics"""
        try:
            async with session.get(self.metrics_url) as response:
                if response.status == 200:
                    metrics_text = await response.text()
                    return self.parse_metrics(metrics_text)
        except Exception as e:
            logger.error(f"Failed to fetch metrics: {e}")
        
        return {}
    
    def parse_metrics(self, metrics_text):
        """Parse Prometheus metrics text"""
        metrics = {}
        for line in metrics_text.split('\n'):
            if line.startswith('mote_') and not line.startswith('#'):
                parts = line.split()
                if len(parts) >= 2:
                    metric_name = parts[0]
                    value = float(parts[1])
                    metrics[metric_name] = value
        return metrics
    
    async def monitor_queue_depth(self, session, duration=60):
        """Monitor embedding queue depth over time"""
        start_time = time.time()
        queue_depths = []
        
        while time.time() - start_time < duration:
            metrics = await self.get_metrics(session)
            queue_depth = metrics.get('mote_embedding_queue_depth', 0)
            queue_depths.append({
                'timestamp': time.time(),
                'depth': queue_depth
            })
            
            # If queue is empty and we have some data, we can stop early
            if queue_depth == 0 and len(queue_depths) > 5:
                logger.info(f"📊 Monitoring complete - queue is empty")
                break
                
            await asyncio.sleep(1)
        
        return queue_depths
    
    async def run_stress_test(self, num_agents=50, atoms_per_batch=20, concurrent_publishers=10):
        """Run the embedding stress test"""
        logger.info(f"🔥 Starting Embedding Queue Stress Test")
        logger.info(f"📊 Configuration: {num_agents} agents, {atoms_per_batch} atoms/batch, {concurrent_publishers} concurrent publishers")
        
        # Create connector for high concurrency
        connector = aiohttp.TCPConnector(
            limit=100,
            limit_per_host=50,
            ttl_dns_cache=300,
            use_dns_cache=True,
        )
        
        timeout = aiohttp.ClientTimeout(total=120, connect=10)
        
        async with aiohttp.ClientSession(connector=connector, timeout=timeout) as session:
            # Phase 1: Create agents
            logger.info("🔐 Creating and registering agents...")
            agent_creation_start = time.time()
            
            agent_tasks = [
                self.create_agent(session, i) 
                for i in range(num_agents)
            ]
            
            agents_results = await asyncio.gather(*agent_tasks, return_exceptions=True)
            agents = [agent for agent in agents_results if agent is not None]
            
            agent_creation_time = time.time() - agent_creation_start
            logger.info(f"✅ Created {len(agents)} agents in {agent_creation_time:.2f}s")
            
            if len(agents) < num_agents // 2:
                logger.error(f"❌ Too few agents created: {len(agents)}/{num_agents}")
                return
            
            # Phase 2: Start queue monitoring
            logger.info("📈 Starting queue depth monitoring...")
            monitor_task = asyncio.create_task(self.monitor_queue_depth(session, duration=120))
            
            # Phase 3: Stress test with concurrent publishers
            logger.info("🚀 Starting stress test with concurrent publishers...")
            stress_start = time.time()
            
            # Create publishing tasks
            publishing_tasks = []
            for i in range(concurrent_publishers):
                agent = random.choice(agents)
                task = asyncio.create_task(
                    self.publish_atoms_batch(session, agent, atoms_per_batch)
                )
                publishing_tasks.append(task)
            
            # Wait for all publishing to complete
            publish_results = await asyncio.gather(*publishing_tasks, return_exceptions=True)
            
            stress_time = time.time() - stress_start
            
            # Analyze publishing results
            successful_publishes = [r for r in publish_results if isinstance(r, tuple) and r[0]]
            failed_publishes = [r for r in publish_results if not (isinstance(r, tuple) and r[0])]
            
            total_atoms_published = sum(r[2] for r in successful_publishes)
            avg_publish_latency = statistics.mean([r[1] for r in successful_publishes]) if successful_publishes else 0
            
            logger.info(f"📊 Publishing completed in {stress_time:.2f}s")
            logger.info(f"✅ Successful publishes: {len(successful_publishes)}/{len(publishing_tasks)}")
            logger.info(f"📦 Total atoms published: {total_atoms_published}")
            logger.info(f"⏱️  Average publish latency: {avg_publish_latency:.3f}s")
            
            # Phase 4: Wait for queue to drain and get final metrics
            logger.info("⏳ Waiting for embedding queue to drain...")
            
            # Wait dynamically for queue to drain (max 30 seconds)
            max_wait_time = 30
            wait_start = time.time()
            while time.time() - wait_start < max_wait_time:
                metrics = await self.get_metrics(session)
                queue_depth = metrics.get('mote_embedding_queue_depth', 0)
                if queue_depth == 0:
                    logger.info(f"✅ Queue drained after {time.time() - wait_start:.1f}s")
                    break
                await asyncio.sleep(1)
            else:
                logger.warning(f"⚠️ Queue did not fully drain after {max_wait_time}s")
            
            # Stop monitoring
            queue_depths = await monitor_task
            
            # Get final metrics
            final_metrics = await self.get_metrics(session)
            
            # Analyze results
            self.analyze_results(
                queue_depths, final_metrics, 
                len(agents), total_atoms_published,
                stress_time, avg_publish_latency
            )
    
    def analyze_results(self, queue_depths, final_metrics, num_agents, total_atoms, stress_time, avg_latency):
        """Analyze and report stress test results"""
        print(f"\n{'='*80}")
        print(f"EMBEDDING QUEUE STRESS TEST RESULTS")
        print(f"{'='*80}")
        
        # Queue depth analysis
        if queue_depths:
            depths = [d['depth'] for d in queue_depths]
            max_depth = max(depths)
            avg_depth = statistics.mean(depths)
            depth_variance = statistics.variance(depths) if len(depths) > 1 else 0
            
            print(f"📊 Queue Depth Analysis:")
            print(f"   Maximum depth: {max_depth}")
            print(f"   Average depth: {avg_depth:.2f}")
            print(f"   Depth variance: {depth_variance:.2f}")
            
            # Check if queue exceeded 32 (worker pool size)
            if max_depth > 32:
                print(f"   ⚠️  Queue exceeded worker pool size (32) by {max_depth - 32}")
            else:
                print(f"   ✅ Queue stayed within worker pool bounds")
        
        # Throughput analysis
        total_time = stress_time
        atoms_per_second = total_atoms / total_time if total_time > 0 else 0
        agents_per_second = num_agents / total_time if total_time > 0 else 0
        
        print(f"\n🚀 Throughput Analysis:")
        print(f"   Total atoms: {total_atoms}")
        print(f"   Total time: {total_time:.2f}s")
        print(f"   Atoms/second: {atoms_per_second:.2f}")
        print(f"   Agents/second: {agents_per_second:.2f}")
        print(f"   Average latency: {avg_latency:.3f}s")
        
        # Server metrics
        print(f"\n📈 Server Metrics:")
        print(f"   Embedding jobs completed: {final_metrics.get('mote_embedding_jobs_total{status=\"completed\"}', 'N/A')}")
        print(f"   Embedding jobs failed: {final_metrics.get('mote_embedding_jobs_total{status=\"failed\"}', 'N/A')}")
        print(f"   Current queue depth: {final_metrics.get('mote_embedding_queue_depth', 'N/A')}")
        print(f"   Publish requests accepted: {final_metrics.get('mote_publish_requests_total{status=\"accepted\"}', 'N/A')}")
        print(f"   Publish requests rejected: {final_metrics.get('mote_publish_requests_total{status=\"rejected\"}', 'N/A')}")
        
        # Performance assessment
        print(f"\n🎯 Performance Assessment:")
        if avg_latency < 1.0:
            print(f"   ✅ Good latency (< 1s)")
        elif avg_latency < 2.0:
            print(f"   ⚠️  Moderate latency (1-2s)")
        else:
            print(f"   ❌ High latency (> 2s)")
        
        if atoms_per_second > 50:
            print(f"   ✅ High throughput (> 50 atoms/s)")
        elif atoms_per_second > 20:
            print(f"   ⚠️  Moderate throughput (20-50 atoms/s)")
        else:
            print(f"   ❌ Low throughput (< 20 atoms/s)")
        
        # Recommendations
        print(f"\n💡 Recommendations:")
        if max_depth > 32:
            print(f"   - Consider increasing embedding worker pool size")
            print(f"   - Implement read replicas for pgvector")
            print(f"   - Consider pgvector partitioning")
        
        if avg_latency > 2.0:
            print(f"   - Optimize embedding generation process")
            print(f"   - Consider batching embedding requests")
        
        if final_metrics.get('mote_embedding_jobs_total{status=\"failed\"}', 0) > 0:
            print(f"   - Investigate embedding job failures")
            print(f"   - Check embedding service availability")
        
        print(f"{'='*80}")

async def main():
    parser = argparse.ArgumentParser(description='Embedding Queue Stress Test')
    parser.add_argument('--agents', type=int, default=50, help='Number of agents')
    parser.add_argument('--atoms-per-batch', type=int, default=20, help='Atoms per batch')
    parser.add_argument('--concurrent-publishers', type=int, default=10, help='Concurrent publishers')
    parser.add_argument('--url', default='http://localhost:3000', help='Mote server URL')
    
    args = parser.parse_args()
    
    stress_test = EmbeddingStressTest(args.url)
    
    try:
        await stress_test.run_stress_test(
            args.agents, 
            args.atoms_per_batch, 
            args.concurrent_publishers
        )
    except Exception as e:
        logger.error(f"Stress test failed: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit_code = asyncio.run(main())
    exit(exit_code)
