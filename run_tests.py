#!/usr/bin/env python3
"""
Comprehensive testing script for Mote exploration system
Sets up test environment, runs all tests, and cleans up
"""

import os
import sys
import subprocess
import tempfile
import shutil
import time
import json
import requests
from pathlib import Path
from typing import Dict, List, Tuple

class TestEnvironment:
    """Manages test environment setup and cleanup"""
    
    def __init__(self):
        self.temp_dir = None
        self.test_db_name = "mote_test_" + str(int(time.time()))
        self.docker_compose_file = "docker-compose.yml"
        self.original_env = {}
        
    def setup(self) -> bool:
        """Setup test environment"""
        print("🔧 Setting up test environment...")
        
        # Store original environment
        self.original_env = os.environ.copy()
        
        # Create temporary directory for test configs
        self.temp_dir = tempfile.mkdtemp(prefix="mote_test_")
        print(f"📁 Created temp directory: {self.temp_dir}")
        
        # Create test config with all required fields
        self._create_test_config()
        
        # Setup test database
        if not self._setup_test_database():
            return False
            
        # Set environment variables for tests
        os.environ["DATABASE_URL"] = f"postgres://mote:mote_password@localhost:5432/{self.test_db_name}"
        
        print("✅ Test environment setup complete")
        return True
        
    def _create_test_config(self):
        """Create test configuration file"""
        config_path = Path(self.temp_dir) / "test_config.toml"
        config_content = """
[hub]
name = "test-hub"
domain = "test.mote"
listen_address = "127.0.0.1:8080"
embedding_endpoint = "http://localhost:11434"
embedding_model = "nomic-embed-text"
embedding_dimension = 768
structured_vector_reserved_dims = 10
dims_per_numeric_key = 2
dims_per_categorical_key = 1
neighbourhood_radius = 0.1
summary_llm_endpoint = "http://localhost:11434"
summary_llm_model = "llama2"
artifact_storage_path = "./test_artifacts"
max_artifact_blob_bytes = 1048576
max_artifact_storage_per_agent_bytes = 10485760

[pheromone]
decay_half_life_hours = 24
attraction_cap = 10.0
novelty_radius = 0.5
disagreement_threshold = 0.8
exploration_samples = 10
exploration_density_radius = 0.5

[trust]
reliability_threshold = 0.7
independence_ancestry_depth = 5
probation_atom_count = 10
max_atoms_per_hour = 100

[workers]
embedding_pool_size = 4
decay_interval_minutes = 60
claim_ttl_hours = 24
staleness_check_interval_minutes = 30
bounty_needed_novelty_threshold = 0.7

[acceptance]
required_provenance_fields = ["agent_id", "timestamp"]

[mcp]
allowed_origins = ["http://localhost:3000", "https://localhost:3000"]
"""
        config_path.write_text(config_content)
        
        # Update only the valid test config file (not the invalid one)
        test_config_path = "tests/test_config.toml"
        if Path(test_config_path).exists():
            shutil.copy(config_path, test_config_path)
        
    def _setup_test_database(self) -> bool:
        """Setup test database"""
        print("🗄️ Setting up test database...")
        
        try:
            # Wait for PostgreSQL to be ready via Docker
            max_attempts = 30
            for attempt in range(max_attempts):
                try:
                    result = subprocess.run([
                        "docker", "exec", "mote-postgres-1", 
                        "psql", "-U", "mote", "-d", "postgres", 
                        "-c", "SELECT 1"
                    ], capture_output=True, text=True, timeout=5)
                    if result.returncode == 0:
                        break
                except Exception:
                    if attempt == max_attempts - 1:
                        print("❌ Failed to connect to PostgreSQL")
                        return False
                    time.sleep(1)
            
            # Create test database
            create_db_cmd = f"CREATE DATABASE {self.test_db_name}"
            result = subprocess.run([
                "docker", "exec", "mote-postgres-1",
                "psql", "-U", "mote", "-d", "postgres",
                "-c", create_db_cmd
            ], capture_output=True, text=True)
            
            if result.returncode != 0:
                print(f"❌ Failed to create database: {result.stderr}")
                return False
            
            # Run migrations on test database
            test_db_url = f"postgres://mote:mote_password@localhost:5432/{self.test_db_name}"
            result = subprocess.run([
                "sqlx", "migrate", "run", "--source", "migrations"
            ], env={**os.environ, "DATABASE_URL": test_db_url}, 
            capture_output=True, text=True)
            
            if result.returncode != 0:
                print(f"❌ Migration failed: {result.stderr}")
                return False
                
            print(f"✅ Test database '{self.test_db_name}' created and migrated")
            return True
            
        except Exception as e:
            print(f"❌ Database setup failed: {e}")
            return False
    
    async def _connect_to_postgres(self, database: str):
        """Connect to PostgreSQL (not used in current implementation)"""
        pass
    
    def cleanup(self):
        """Cleanup test environment"""
        print("🧹 Cleaning up test environment...")
        
        # Restore original environment
        os.environ.clear()
        os.environ.update(self.original_env)
        
        # Cleanup test database
        try:
            drop_db_cmd = f"DROP DATABASE IF EXISTS {self.test_db_name}"
            result = subprocess.run([
                "docker", "exec", "mote-postgres-1",
                "psql", "-U", "mote", "-d", "postgres",
                "-c", drop_db_cmd
            ], capture_output=True, text=True)
            
            if result.returncode == 0:
                print(f"✅ Dropped test database '{self.test_db_name}'")
            else:
                print(f"⚠️ Failed to drop test database: {result.stderr}")
        except Exception as e:
            print(f"⚠️ Failed to drop test database: {e}")
        
        # Cleanup temp directory
        if self.temp_dir and Path(self.temp_dir).exists():
            shutil.rmtree(self.temp_dir)
            print(f"✅ Cleaned up temp directory")
        
        print("✅ Cleanup complete")

class TestRunner:
    """Runs all tests and reports results"""
    
    def __init__(self, env: TestEnvironment):
        self.env = env
        self.results = {}
        
    def run_all_tests(self) -> Dict[str, bool]:
        """Run all tests"""
        print("🚀 Running comprehensive test suite...")
        print("=" * 60)
        
        # 1. Rust Library Tests
        self.results["rust_library"] = self._run_rust_library_tests()
        
        # 2. Rust Integration Tests  
        self.results["rust_integration"] = self._run_rust_integration_tests()
        
        # 3. Rust Config Tests
        self.results["rust_config"] = self._run_rust_config_tests()
        
        # 4. Python Direct Database Tests
        self.results["python_database"] = self._run_python_database_tests()
        
        # 5. Python RPC Tests
        self.results["python_rpc"] = self._run_python_rpc_tests()
        
        # 6. Python MCP Tests
        self.results["python_mcp"] = self._run_python_mcp_tests()
        
        # 7. API Endpoint Tests
        self.results["api_endpoints"] = self._run_api_endpoint_tests()
        
        return self.results
    
    def _run_rust_library_tests(self) -> bool:
        """Run Rust library tests"""
        print("\n📚 Running Rust library tests...")
        try:
            project_dir = Path(__file__).parent
            result = subprocess.run([
                "cargo", "test", "--lib"
            ], cwd=project_dir, capture_output=True, text=True, timeout=300)
            
            success = result.returncode == 0
            if success:
                print("✅ Rust library tests passed")
            else:
                print("❌ Rust library tests failed")
                print(result.stderr[:500])  # Show first 500 chars of error
                
            return success
        except subprocess.TimeoutExpired:
            print("❌ Rust library tests timed out")
            return False
        except Exception as e:
            print(f"❌ Rust library test error: {e}")
            return False
    
    def _run_rust_integration_tests(self) -> bool:
        """Run Rust integration tests"""
        print("\n🔗 Running Rust integration tests...")
        try:
            project_dir = Path(__file__).parent
            result = subprocess.run([
                "cargo", "test", "--test", "integration"
            ], cwd=project_dir, env={**os.environ, "DATABASE_URL": f"postgres://mote:mote_password@localhost:5432/{self.env.test_db_name}"},
            capture_output=True, text=True, timeout=600)
            
            success = result.returncode == 0
            if success:
                print("✅ Rust integration tests passed")
            else:
                print("❌ Rust integration tests failed")
                print(result.stderr[:500])
                
            return success
        except subprocess.TimeoutExpired:
            print("❌ Rust integration tests timed out")
            return False
        except Exception as e:
            print(f"❌ Rust integration test error: {e}")
            return False
    
    def _run_rust_config_tests(self) -> bool:
        """Run Rust config tests"""
        print("\n⚙️ Running Rust config tests...")
        try:
            # Change to the project directory to run config tests
            project_dir = Path(__file__).parent
            result = subprocess.run([
                "cargo", "test", "--test", "config_tests"
            ], cwd=project_dir, capture_output=True, text=True, timeout=120)
            
            success = result.returncode == 0
            if success:
                print("✅ Rust config tests passed")
            else:
                print("❌ Rust config tests failed")
                print(result.stderr[:500])
                
            return success
        except subprocess.TimeoutExpired:
            print("❌ Rust config tests timed out")
            return False
        except Exception as e:
            print(f"❌ Rust config test error: {e}")
            return False
    
    def _run_python_database_tests(self) -> bool:
        """Run Python direct database tests"""
        print("\n🗄️ Running Python database tests...")
        
        test_files = [
            "tests/mcp-py-tests/database/test_exploration_direct.py"
        ]
        
        for test_file in test_files:
            if Path(test_file).exists():
                try:
                    result = subprocess.run([
                        "python3", test_file
                    ], capture_output=True, text=True, timeout=60)
                    
                    if result.returncode == 0:
                        print(f"✅ {test_file} passed")
                    else:
                        print(f"❌ {test_file} failed")
                        print(result.stderr[:300])
                        return False
                except subprocess.TimeoutExpired:
                    print(f"❌ {test_file} timed out")
                    return False
                except Exception as e:
                    print(f"❌ {test_file} error: {e}")
                    return False
            else:
                print(f"⚠️ {test_file} not found, skipping")
        
        return True
    
    def _run_python_rpc_tests(self) -> bool:
        """Run Python RPC tests"""
        print("\n🔌 Running Python RPC tests...")
        
        test_files = [
            "tests/mcp-py-tests/api/test_rpc_direct.py"
        ]
        
        for test_file in test_files:
            if Path(test_file).exists():
                try:
                    result = subprocess.run([
                        "python3", test_file
                    ], capture_output=True, text=True, timeout=60)
                    
                    if result.returncode == 0:
                        print(f"✅ {test_file} passed")
                    else:
                        print(f"❌ {test_file} failed")
                        print(result.stderr[:300])
                        return False
                except subprocess.TimeoutExpired:
                    print(f"❌ {test_file} timed out")
                    return False
                except Exception as e:
                    print(f"❌ {test_file} error: {e}")
                    return False
            else:
                print(f"⚠️ {test_file} not found, skipping")
        
        return True
    
    def _run_python_mcp_tests(self) -> bool:
        """Run Python MCP tests"""
        print("\n🔗 Running Python MCP tests...")
        
        test_files = [
            "tests/mcp-py-tests/exploration/test_exploration_functionality.py"
        ]
        
        for test_file in test_files:
            if Path(test_file).exists():
                try:
                    result = subprocess.run([
                        "python3", test_file
                    ], capture_output=True, text=True, timeout=60)
                    
                    if result.returncode == 0:
                        print(f"✅ {test_file} passed")
                    else:
                        print(f"❌ {test_file} failed")
                        print(result.stderr[:300])
                        return False
                except subprocess.TimeoutExpired:
                    print(f"❌ {test_file} timed out")
                    return False
                except Exception as e:
                    print(f"❌ {test_file} error: {e}")
                    return False
            else:
                print(f"⚠️ {test_file} not found, skipping")
        
        return True
    
    def _run_api_endpoint_tests(self) -> bool:
        """Run API endpoint tests"""
        print("\n🌐 Running API endpoint tests...")
        
        try:
            # Test health endpoint
            response = requests.get("http://localhost:3000/health", timeout=10)
            if response.status_code != 200:
                print("❌ Health endpoint failed")
                return False
            print("✅ Health endpoint working")
            
            # Test RPC endpoint
            rpc_response = requests.post("http://localhost:3000/rpc", 
                json={
                    "jsonrpc": "2.0",
                    "method": "register_agent_simple",
                    "params": {"agent_name": "test-runner"},
                    "id": 1
                }, timeout=10)
                
            if rpc_response.status_code != 200:
                print("❌ RPC endpoint failed")
                return False
            print("✅ RPC endpoint working")
            
            # Test exploration functionality
            agent_data = rpc_response.json()
            agent_id = agent_data.get("result", {}).get("agent_id")
            api_token = agent_data.get("result", {}).get("api_token")
            
            if not agent_id or not api_token:
                print("❌ Failed to get agent credentials")
                return False
            
            exploration_response = requests.post("http://localhost:3000/rpc",
                json={
                    "jsonrpc": "2.0",
                    "method": "get_suggestions",
                    "params": {
                        "agent_id": agent_id,
                        "api_token": api_token,
                        "limit": 5,
                        "include_exploration": True
                    },
                    "id": 2
                }, timeout=10)
            
            if exploration_response.status_code != 200:
                print("❌ Exploration RPC failed")
                return False
                
            exploration_data = exploration_response.json()
            strategy = exploration_data.get("result", {}).get("strategy")
            
            if strategy == "pheromone_attraction_plus_exploration":
                print("✅ Exploration mode working")
            else:
                print(f"❌ Exploration mode failed: strategy = {strategy}")
                return False
            
            return True
            
        except Exception as e:
            print(f"❌ API endpoint test error: {e}")
            return False
    
    def print_results(self):
        """Print test results summary"""
        print("\n" + "=" * 60)
        print("📊 TEST RESULTS SUMMARY")
        print("=" * 60)
        
        passed = 0
        total = len(self.results)
        
        for test_name, result in self.results.items():
            status = "✅ PASS" if result else "❌ FAIL"
            print(f"{status:<8} {test_name.replace('_', ' ').title()}")
            if result:
                passed += 1
        
        print("-" * 60)
        print(f"📈 Overall: {passed}/{total} tests passed")
        
        if passed == total:
            print("🎉 ALL TESTS PASSED! The exploration system is ready!")
        else:
            print("⚠️ Some tests failed. Check the logs above for details.")
        
        print("=" * 60)

def main():
    """Main test runner"""
    print("🧪 Mote Exploration System - Comprehensive Test Suite")
    print("=" * 60)
    
    env = TestEnvironment()
    
    try:
        # Setup test environment
        if not env.setup():
            print("❌ Failed to setup test environment")
            return 1
        
        # Run all tests
        runner = TestRunner(env)
        results = runner.run_all_tests()
        
        # Print results
        runner.print_results()
        
        # Return exit code based on results
        return 0 if all(results.values()) else 1
        
    except KeyboardInterrupt:
        print("\n⚠️ Tests interrupted by user")
        return 1
    except Exception as e:
        print(f"\n❌ Unexpected error: {e}")
        return 1
    finally:
        # Cleanup
        env.cleanup()

if __name__ == "__main__":
    sys.exit(main())
