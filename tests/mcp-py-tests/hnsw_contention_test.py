#!/usr/bin/env python3
"""
pgvector HNSW Contention Test
Tests vector similarity search under high concurrent load
Identifies bottlenecks in HNSW index operations
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
import numpy as np
import argparse

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

class HNSWContentionTest:
    def __init__(self, base_url="http://localhost:3000"):
        self.base_url = base_url
        self.mcp_url = f"{base_url}/mcp"
        self.metrics_url = f"{base_url}/metrics"
        self.agents = []
        
    async def create_agent(self, session, agent_id):
        """Create and register an agent"""
        private_key = ed25519.Ed25519PrivateKey.generate()
        public_key = private_key.public_key()
        
        # Register agent
        reg_payload = {
            "jsonrpc": "2.0",
            "method": "register_agent",
            "params": {
                "public_key": binascii.hexlify(public_key.public_bytes_raw()).decode()
            },
            "id": agent_id * 1000
        }
        
        async with session.post(self.mcp_url, json=reg_payload) as response:
            result = await response.json()
            if 'result' in result:
                agent_info = {
                    'agent_id': result['result']['agent_id'],
                    'challenge': result['result']['challenge'],
                    'private_key': private_key
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
                    "id": agent_id * 1000 + 1
                }
                
                async with session.post(self.mcp_url, json=conf_payload) as conf_response:
                    conf_result = await conf_response.json()
                    if 'result' in conf_result and conf_result['result']['status'] == 'confirmed':
                        return agent_info
        
        return None
    
    async def publish_vector_atoms(self, session, agent, count=50):
        """Publish atoms with vector embeddings to populate HNSW index"""
        atoms = []
        
        for i in range(count):
            # Generate deterministic vector based on agent_id and i
            vector_seed = hash(f"{agent['agent_id']}_{i}") % 1000000
            np.random.seed(vector_seed)
            embedding = np.random.random(1536).tolist()  # Match embedding dimension
            
            # Create atom signature (mock for testing)
            atom_signature = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
            atom_signature_bytes = binascii.unhexlify(atom_signature)
            
            atom_data = {
                "atom_type": random.choice(["finding", "hypothesis"]),
                "domain": random.choice(["machine_learning", "nlp", "computer_vision"]),
                "statement": f"Vector-based research result {i} with embedding dimension 1536",
                "conditions": {
                    "vector_id": i,
                    "cluster_id": i % 10,  # Create 10 clusters
                    "has_embedding": True
                },
                "metrics": {
                    "similarity_score": round(random.uniform(0.7, 0.95), 4),
                    "cluster_confidence": round(random.uniform(0.8, 0.99), 4),
                    "embedding_norm": round(np.linalg.norm(embedding), 4)
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
        
        start_time = time.time()
        async with session.post(self.mcp_url, json=payload) as response:
            result = await response.json()
            latency = time.time() - start_time
            
            if 'result' in result:
                atom_ids = result['result'].get('published_atoms', [])  # Fixed: was 'atom_ids'
                return True, latency, len(atom_ids)
            else:
                return False, latency, 0
    
    async def search_vectors(self, session, agent, search_count=20):
        """Perform vector similarity searches"""
        search_latencies = []
        successful_searches = 0
        
        for i in range(search_count):
            # Generate query vector
            query_seed = hash(f"search_{agent['agent_id']}_{i}") % 1000000
            np.random.seed(query_seed)
            query_embedding = np.random.random(1536).tolist()
            
            payload = {
                "jsonrpc": "2.0",
                "method": "search_atoms",
                "params": {
                    "domain": random.choice(["machine_learning", "nlp", "computer_vision"]),
                    "atom_types": ["finding", "hypothesis"],
                    "limit": 10,
                    "embedding_query": query_embedding  # This would trigger vector search
                },
                "id": int(time.time() * 1000 + i) % 1000000
            }
            
            start_time = time.time()
            try:
                async with session.post(self.mcp_url, json=payload, timeout=aiohttp.ClientTimeout(total=30)) as response:
                    result = await response.json()
                    latency = time.time() - start_time
                    search_latencies.append(latency)
                    
                    if response.status == 200 and 'result' in result:
                        successful_searches += 1
                    else:
                        logger.warning(f"Search failed: {result}")
                        
            except asyncio.TimeoutError:
                latency = time.time() - start_time
                search_latencies.append(latency)
                logger.warning(f"Search timeout after {latency:.2f}s")
            except Exception as e:
                latency = time.time() - start_time
                search_latencies.append(latency)
                logger.warning(f"Search error: {e}")
        
        return successful_searches, search_latencies
    
    async def get_suggestions_load(self, session, agent, suggestion_count=15):
        """Test suggestion system under load"""
        suggestion_latencies = []
        successful_suggestions = 0
        
        for i in range(suggestion_count):
            payload = {
                "jsonrpc": "2.0",
                "method": "get_suggestions",
                "params": {
                    "context": {
                        "domain": random.choice(["machine_learning", "nlp", "computer_vision"]),
                        "agent_preferences": {
                            "risk_tolerance": random.uniform(0.1, 0.9),
                            "expertise_level": random.choice(["beginner", "intermediate", "expert"])
                        }
                    },
                    "k": random.randint(5, 25)
                },
                "id": int(time.time() * 1000 + i) % 1000000
            }
            
            start_time = time.time()
            try:
                async with session.post(self.mcp_url, json=payload, timeout=aiohttp.ClientTimeout(total=30)) as response:
                    result = await response.json()
                    latency = time.time() - start_time
                    suggestion_latencies.append(latency)
                    
                    if response.status == 200 and 'result' in result:
                        suggestions = result['result'].get('suggestions', [])
                        successful_suggestions += len(suggestions)
                    else:
                        logger.warning(f"Suggestion failed: {result}")
                        
            except asyncio.TimeoutError:
                latency = time.time() - start_time
                suggestion_latencies.append(latency)
                logger.warning(f"Suggestion timeout after {latency:.2f}s")
            except Exception as e:
                latency = time.time() - start_time
                suggestion_latencies.append(latency)
                logger.warning(f"Suggestion error: {e}")
        
        return successful_suggestions, suggestion_latencies
    
    async def monitor_database_metrics(self, session, duration=60):
        """Monitor database-specific metrics"""
        start_time = time.time()
        metrics_samples = []
        
        while time.time() - start_time < duration:
            try:
                async with session.get(self.metrics_url) as response:
                    if response.status == 200:
                        metrics_text = await response.text()
                        metrics_sample = {
                            'timestamp': time.time(),
                            'metrics': self.parse_metrics(metrics_text)
                        }
                        metrics_samples.append(metrics_sample)
            except Exception as e:
                logger.error(f"Failed to get metrics: {e}")
            
            # Stop early if we have enough data and activity has ceased
            if len(metrics_samples) > 10:
                recent_queue_depths = [m['metrics'].get('mote_embedding_queue_depth', 0) for m in metrics_samples[-5:]]
                if all(depth == 0 for depth in recent_queue_depths):
                    logger.info(f"📊 Database monitoring complete - activity ceased")
                    break
                    
            await asyncio.sleep(2)
        
        return metrics_samples
    
    async def get_metrics(self, session):
        """Fetch current server metrics"""
        try:
            async with session.get(self.metrics_url) as response:
                if response.status == 200:
                    metrics_text = await response.text()
                    return self.parse_metrics(metrics_text)
        except Exception as e:
            logger.error(f"Failed to get metrics: {e}")
        return {}
    
    def parse_metrics(self, metrics_text):
        """Parse Prometheus metrics text"""
        metrics = {}
        for line in metrics_text.split('\n'):
            if line.startswith('mote_') and not line.startswith('#'):
                parts = line.split()
                if len(parts) >= 2:
                    metric_name = parts[0]
                    try:
                        value = float(parts[1])
                        metrics[metric_name] = value
                    except ValueError:
                        continue
        return metrics
    
    async def run_contention_test(self, num_agents=30, atoms_per_agent=100, concurrent_searchers=20):
        """Run the HNSW contention test"""
        logger.info(f"🔥 Starting pgvector HNSW Contention Test")
        logger.info(f"📊 Configuration: {num_agents} agents, {atoms_per_agent} atoms/agent, {concurrent_searchers} concurrent searchers")
        
        # Create connector for high concurrency
        connector = aiohttp.TCPConnector(
            limit=150,
            limit_per_host=75,
            ttl_dns_cache=300,
            use_dns_cache=True,
        )
        
        timeout = aiohttp.ClientTimeout(total=180, connect=15)
        
        async with aiohttp.ClientSession(connector=connector, timeout=timeout) as session:
            # Phase 1: Create agents
            logger.info("🔐 Creating and registering agents...")
            agent_tasks = [self.create_agent(session, i) for i in range(num_agents)]
            agents_results = await asyncio.gather(*agent_tasks, return_exceptions=True)
            agents = [agent for agent in agents_results if agent is not None]
            
            logger.info(f"✅ Created {len(agents)} agents")
            
            if len(agents) < num_agents // 2:
                logger.error(f"❌ Too few agents created: {len(agents)}/{num_agents}")
                return
            
            # Phase 2: Populate HNSW index with vectors
            logger.info("📦 Populating HNSW index with vector atoms...")
            populate_start = time.time()
            
            populate_tasks = [
                self.publish_vector_atoms(session, agent, atoms_per_agent)
                for agent in agents
            ]
            
            populate_results = await asyncio.gather(*populate_tasks, return_exceptions=True)
            populate_time = time.time() - populate_start
            
            successful_populates = [r for r in populate_results if isinstance(r, tuple) and r[0]]
            total_atoms_populated = sum(r[2] for r in successful_populates)
            avg_populate_latency = statistics.mean([r[1] for r in successful_populates]) if successful_populates else 0
            
            logger.info(f"✅ Populated {total_atoms_populated} atoms in {populate_time:.2f}s")
            logger.info(f"⏱️  Average populate latency: {avg_populate_latency:.3f}s")
            
            # Wait for embeddings to be processed (dynamically)
            logger.info("⏳ Waiting for embeddings to be processed...")
            max_wait_time = 30
            wait_start = time.time()
            while time.time() - wait_start < max_wait_time:
                metrics = await self.get_metrics(session)
                queue_depth = metrics.get('mote_embedding_queue_depth', 0)
                if queue_depth == 0:
                    logger.info(f"✅ Embeddings processed after {time.time() - wait_start:.1f}s")
                    break
                await asyncio.sleep(1)
            else:
                logger.warning(f"⚠️ Embeddings still processing after {max_wait_time}s")
            
            # Phase 3: Start database monitoring
            logger.info("📈 Starting database metrics monitoring...")
            monitor_task = asyncio.create_task(self.monitor_database_metrics(session, duration=120))
            
            # Phase 4: Concurrent search contention test
            logger.info("🔍 Starting concurrent search contention test...")
            search_start = time.time()
            
            # Create concurrent search tasks
            search_tasks = []
            for i in range(concurrent_searchers):
                agent = random.choice(agents)
                # Mix of vector searches and suggestions
                if i % 2 == 0:
                    task = asyncio.create_task(self.search_vectors(session, agent, 25))
                else:
                    task = asyncio.create_task(self.get_suggestions_load(session, agent, 20))
                search_tasks.append(task)
            
            # Execute all search tasks
            search_results = await asyncio.gather(*search_tasks, return_exceptions=True)
            search_time = time.time() - search_start
            
            # Analyze search results
            all_search_latencies = []
            all_suggestion_latencies = []
            total_successful_searches = 0
            total_successful_suggestions = 0
            
            for result in search_results:
                if isinstance(result, tuple) and len(result) == 2:
                    successful_count, latencies = result
                    if latencies and len(latencies) > 0:
                        # Determine if this was a search or suggestion based on average latency
                        avg_latency = statistics.mean(latencies)
                        if avg_latency > 0.5:  # Suggestions typically take longer
                            all_suggestion_latencies.extend(latencies)
                            total_successful_suggestions += successful_count
                        else:
                            all_search_latencies.extend(latencies)
                            total_successful_searches += successful_count
            
            # Stop monitoring
            metrics_samples = await monitor_task
            
            # Analyze and report results
            self.analyze_contention_results(
                metrics_samples,
                total_atoms_populated,
                all_search_latencies,
                all_suggestion_latencies,
                total_successful_searches,
                total_successful_suggestions,
                search_time,
                populate_time
            )
    
    def analyze_contention_results(self, metrics_samples, total_atoms, search_latencies, 
                                 suggestion_latencies, successful_searches, successful_suggestions,
                                 search_time, populate_time):
        """Analyze and report HNSW contention test results"""
        print(f"\n{'='*80}")
        print(f"HNSW CONTENTION TEST RESULTS")
        print(f"{'='*80}")
        
        # Index population analysis
        print(f"📦 Index Population:")
        print(f"   Total atoms: {total_atoms}")
        print(f"   Population time: {populate_time:.2f}s")
        print(f"   Population rate: {total_atoms/populate_time:.2f} atoms/s")
        
        # Search performance analysis
        if search_latencies:
            search_stats = {
                'count': len(search_latencies),
                'avg': statistics.mean(search_latencies),
                'min': min(search_latencies),
                'max': max(search_latencies),
                'p95': statistics.quantiles(search_latencies, n=20)[18] if len(search_latencies) > 20 else max(search_latencies),
                'p99': statistics.quantiles(search_latencies, n=100)[98] if len(search_latencies) > 100 else max(search_latencies)
            }
            
            print(f"\n🔍 Vector Search Performance:")
            print(f"   Total searches: {search_stats['count']}")
            print(f"   Successful: {successful_searches}")
            print(f"   Average latency: {search_stats['avg']:.3f}s")
            print(f"   Min latency: {search_stats['min']:.3f}s")
            print(f"   Max latency: {search_stats['max']:.3f}s")
            print(f"   P95 latency: {search_stats['p95']:.3f}s")
            print(f"   P99 latency: {search_stats['p99']:.3f}s")
            print(f"   Search rate: {search_stats['count']/search_time:.2f} searches/s")
        
        # Suggestion performance analysis
        if suggestion_latencies:
            suggestion_stats = {
                'count': len(suggestion_latencies),
                'avg': statistics.mean(suggestion_latencies),
                'min': min(suggestion_latencies),
                'max': max(suggestion_latencies),
                'p95': statistics.quantiles(suggestion_latencies, n=20)[18] if len(suggestion_latencies) > 20 else max(suggestion_latencies),
                'p99': statistics.quantiles(suggestion_latencies, n=100)[98] if len(suggestion_latencies) > 100 else max(suggestion_latencies)
            }
            
            print(f"\n💡 Suggestion Performance:")
            print(f"   Total suggestions: {suggestion_stats['count']}")
            print(f"   Successful: {successful_suggestions}")
            print(f"   Average latency: {suggestion_stats['avg']:.3f}s")
            print(f"   Min latency: {suggestion_stats['min']:.3f}s")
            print(f"   Max latency: {suggestion_stats['max']:.3f}s")
            print(f"   P95 latency: {suggestion_stats['p95']:.3f}s")
            print(f"   P99 latency: {suggestion_stats['p99']:.3f}s")
            print(f"   Suggestion rate: {suggestion_stats['count']/search_time:.2f} suggestions/s")
        
        # Database metrics analysis
        if metrics_samples:
            print(f"\n📊 Database Metrics Analysis:")
            
            # Extract key metrics over time
            queue_depths = []
            embedding_completed = []
            embedding_failed = []
            
            for sample in metrics_samples:
                metrics = sample['metrics']
                queue_depths.append(metrics.get('mote_embedding_queue_depth', 0))
                embedding_completed.append(metrics.get('mote_embedding_jobs_total{status=\"completed\"}', 0))
                embedding_failed.append(metrics.get('mote_embedding_jobs_total{status=\"failed\"}', 0))
            
            if queue_depths:
                print(f"   Max queue depth: {max(queue_depths)}")
                print(f"   Avg queue depth: {statistics.mean(queue_depths):.2f}")
            
            if embedding_completed:
                completed_during_test = max(embedding_completed) - min(embedding_completed)
                print(f"   Embeddings completed during test: {completed_during_test}")
            
            if embedding_failed:
                failed_during_test = max(embedding_failed) - min(embedding_failed)
                print(f"   Embeddings failed during test: {failed_during_test}")
                failure_rate = failed_during_test / completed_during_test if completed_during_test > 0 else 0
                print(f"   Embedding failure rate: {failure_rate:.2%}")
        
        # Performance assessment
        print(f"\n🎯 Performance Assessment:")
        
        # Search performance assessment
        if search_latencies:
            avg_search_latency = statistics.mean(search_latencies)
            if avg_search_latency < 0.1:
                print(f"   ✅ Excellent search latency (< 0.1s)")
            elif avg_search_latency < 0.5:
                print(f"   ✅ Good search latency (< 0.5s)")
            elif avg_search_latency < 1.0:
                print(f"   ⚠️  Moderate search latency (0.5-1s)")
            else:
                print(f"   ❌ Poor search latency (> 1s)")
        
        # Suggestion performance assessment
        if suggestion_latencies:
            avg_suggestion_latency = statistics.mean(suggestion_latencies)
            if avg_suggestion_latency < 0.5:
                print(f"   ✅ Excellent suggestion latency (< 0.5s)")
            elif avg_suggestion_latency < 1.0:
                print(f"   ✅ Good suggestion latency (< 1s)")
            elif avg_suggestion_latency < 2.0:
                print(f"   ⚠️  Moderate suggestion latency (1-2s)")
            else:
                print(f"   ❌ Poor suggestion latency (> 2s)")
        
        # Contention indicators
        print(f"\n⚠️  Contention Indicators:")
        
        if search_latencies:
            search_p99 = statistics.quantiles(search_latencies, n=100)[98] if len(search_latencies) > 100 else max(search_latencies)
            search_p95 = statistics.quantiles(search_latencies, n=20)[18] if len(search_latencies) > 20 else max(search_latencies)
            
            # High P99/P95 ratio indicates contention
            ratio = search_p99 / search_p95 if search_p95 > 0 else 0
            if ratio > 5.0:
                print(f"   🔴 High search contention detected (P99/P95 ratio: {ratio:.1f})")
            elif ratio > 3.0:
                print(f"   🟡 Moderate search contention (P99/P95 ratio: {ratio:.1f})")
            else:
                print(f"   🟢 Low search contention (P99/P95 ratio: {ratio:.1f})")
        
        # Recommendations
        print(f"\n💡 Recommendations:")
        
        if search_latencies and statistics.mean(search_latencies) > 1.0:
            print(f"   - Consider pgvector read replicas for search operations")
            print(f"   - Implement HNSW index partitioning by domain")
            print(f"   - Optimize HNSW parameters (M, ef_construction, ef)")
        
        if metrics_samples and max([s['metrics'].get('mote_embedding_queue_depth', 0) for s in metrics_samples]) > 50:
            print(f"   - Increase embedding worker pool size")
            print(f"   - Implement embedding request batching")
        
        if suggestion_latencies and statistics.mean(suggestion_latencies) > 2.0:
            print(f"   - Cache suggestion results")
            print(f"   - Pre-compute suggestion clusters")
        
        print(f"{'='*80}")

async def main():
    parser = argparse.ArgumentParser(description='pgvector HNSW Contention Test')
    parser.add_argument('--agents', type=int, default=30, help='Number of agents')
    parser.add_argument('--atoms-per-agent', type=int, default=100, help='Atoms per agent')
    parser.add_argument('--concurrent-searchers', type=int, default=20, help='Concurrent searchers')
    parser.add_argument('--url', default='http://localhost:3000', help='Mote server URL')
    
    args = parser.parse_args()
    
    contention_test = HNSWContentionTest(args.url)
    
    try:
        await contention_test.run_contention_test(
            args.agents,
            args.atoms_per_agent,
            args.concurrent_searchers
        )
    except Exception as e:
        logger.error(f"Contention test failed: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit_code = asyncio.run(main())
    exit(exit_code)
