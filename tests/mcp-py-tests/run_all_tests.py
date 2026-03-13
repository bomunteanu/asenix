#!/usr/bin/env python3
"""
Comprehensive Mote Test Suite Runner
Runs all load tests and generates performance reports
"""
import asyncio
import subprocess
import sys
import time
import json
from datetime import datetime
from pathlib import Path

class TestRunner:
    def __init__(self):
        self.test_results = {}
        self.start_time = None
        self.end_time = None
        
    def run_command(self, cmd, description):
        """Run a command and capture its output"""
        print(f"\n{'='*60}")
        print(f"🧪 Running: {description}")
        print(f"📝 Command: {cmd}")
        print(f"{'='*60}")
        
        start_time = time.time()
        
        try:
            result = subprocess.run(
                cmd,
                shell=True,
                capture_output=True,
                text=True,
                timeout=600  # 10 minute timeout
            )
            
            end_time = time.time()
            duration = end_time - start_time
            
            success = result.returncode == 0
            
            self.test_results[description] = {
                'command': cmd,
                'success': success,
                'duration': duration,
                'stdout': result.stdout,
                'stderr': result.stderr,
                'return_code': result.returncode
            }
            
            if success:
                print(f"✅ {description} completed successfully in {duration:.2f}s")
            else:
                print(f"❌ {description} failed after {duration:.2f}s")
                print(f"📄 Error output:\n{result.stderr}")
            
            return success
            
        except subprocess.TimeoutExpired:
            print(f"⏰ {description} timed out after 10 minutes")
            self.test_results[description] = {
                'command': cmd,
                'success': False,
                'duration': 600,
                'stdout': '',
                'stderr': 'Test timed out after 10 minutes',
                'return_code': -1
            }
            return False
        except Exception as e:
            print(f"💥 {description} crashed: {e}")
            self.test_results[description] = {
                'command': cmd,
                'success': False,
                'duration': 0,
                'stdout': '',
                'stderr': str(e),
                'return_code': -2
            }
            return False
    
    def check_server_health(self):
        """Check if Mote server is running and healthy"""
        print("🏥 Checking Mote server health...")
        
        try:
            import requests
            response = requests.get("http://localhost:3000/health", timeout=5)
            if response.status_code == 200:
                health_data = response.json()
                print(f"✅ Server healthy: {health_data.get('status', 'unknown')}")
                print(f"📊 Database: {health_data.get('database', 'unknown')}")
                print(f"🔗 Graph nodes: {health_data.get('graph_nodes', 0)}")
                print(f"🔗 Graph edges: {health_data.get('graph_edges', 0)}")
                print(f"📦 Embedding queue depth: {health_data.get('embedding_queue_depth', 0)}")
                return True
            else:
                print(f"❌ Server returned status {response.status_code}")
                return False
        except Exception as e:
            print(f"❌ Server health check failed: {e}")
            return False
    
    def generate_report(self):
        """Generate a comprehensive test report"""
        print(f"\n{'='*80}")
        print(f"📊 COMPREHENSIVE TEST REPORT")
        print(f"{'='*80}")
        
        total_tests = len(self.test_results)
        successful_tests = sum(1 for result in self.test_results.values() if result['success'])
        failed_tests = total_tests - successful_tests
        
        total_duration = sum(result['duration'] for result in self.test_results.values())
        
        print(f"📈 Summary:")
        print(f"   Total tests: {total_tests}")
        print(f"   Successful: {successful_tests}")
        print(f"   Failed: {failed_tests}")
        print(f"   Success rate: {successful_tests/total_tests*100:.1f}%")
        print(f"   Total duration: {total_duration:.2f}s")
        
        if self.start_time and self.end_time:
            suite_duration = self.end_time - self.start_time
            print(f"   Suite duration: {suite_duration.total_seconds():.2f}s")
        
        print(f"\n📋 Test Results:")
        for test_name, result in self.test_results.items():
            status = "✅ PASS" if result['success'] else "❌ FAIL"
            print(f"   {status} {test_name} ({result['duration']:.2f}s)")
        
        # Failed tests details
        if failed_tests > 0:
            print(f"\n❌ Failed Tests Details:")
            for test_name, result in self.test_results.items():
                if not result['success']:
                    print(f"\n📝 {test_name}:")
                    print(f"   Return code: {result['return_code']}")
                    print(f"   Error: {result['stderr'][:200]}...")
        
        # Save report to file
        report_data = {
            'timestamp': datetime.now().isoformat(),
            'summary': {
                'total_tests': total_tests,
                'successful_tests': successful_tests,
                'failed_tests': failed_tests,
                'success_rate': successful_tests/total_tests*100,
                'total_duration': total_duration,
                'suite_duration': (self.end_time - self.start_time).total_seconds() if self.start_time and self.end_time else None
            },
            'test_results': self.test_results
        }
        
        report_file = Path("test_report.json")
        with open(report_file, 'w') as f:
            json.dump(report_data, f, indent=2)
        
        print(f"\n📄 Detailed report saved to: {report_file}")
        print(f"{'='*80}")
        
        return successful_tests == total_tests
    
    async def run_all_tests(self):
        """Run all test suites"""
        self.start_time = datetime.now()
        
        print(f"🚀 Starting Mote Comprehensive Test Suite")
        print(f"📅 Started at: {self.start_time.isoformat()}")
        
        # Check server health first
        if not self.check_server_health():
            print("❌ Server is not healthy. Please start Mote server before running tests.")
            return False
        
        # Install required Python packages
        print("\n📦 Installing required packages...")
        packages_success = self.run_command(
            "pip3 install aiohttp cryptography numpy",
            "Install Python dependencies"
        )
        
        if not packages_success:
            print("❌ Failed to install dependencies")
            return False
        
        # Test 1: Basic functionality test
        basic_success = self.run_command(
            "python3 tests/mcp-py-tests/mcp-test.py",
            "Basic functionality test"
        )
        
        if not basic_success:
            print("⚠️ Basic test failed, but continuing with load tests...")
        
        # Test 2: Load test with 100 agents
        load_success = self.run_command(
            "python3 tests/mcp-py-tests/load_test.py --agents 100 --operations 10 --batches 5",
            "Load test (100 agents, 10 ops each)"
        )
        
        # Test 3: Embedding queue stress test
        embedding_success = self.run_command(
            "python3 tests/mcp-py-tests/embedding_stress_test.py --agents 50 --atoms-per-batch 20 --concurrent-publishers 10",
            "Embedding queue stress test"
        )
        
        # Test 4: HNSW contention test
        hnsw_success = self.run_command(
            "python3 tests/mcp-py-tests/hnsw_contention_test.py --agents 30 --atoms-per-agent 50 --concurrent-searchers 15",
            "pgvector HNSW contention test"
        )
        
        # Test 5: High-intensity load test
        intense_success = self.run_command(
            "python3 tests/mcp-py-tests/load_test.py --agents 200 --operations 20 --batches 8",
            "High-intensity load test (200 agents)"
        )
        
        self.end_time = datetime.now()
        
        # Generate final report
        all_passed = self.generate_report()
        
        if all_passed:
            print("\n🎉 All tests passed! Mote system is performing excellently.")
        else:
            print("\n⚠️ Some tests failed. Review the report for details.")
        
        return all_passed

def main():
    """Main entry point"""
    runner = TestRunner()
    
    try:
        success = asyncio.run(runner.run_all_tests())
        return 0 if success else 1
    except KeyboardInterrupt:
        print("\n⏹️ Tests interrupted by user")
        return 130
    except Exception as e:
        print(f"\n💥 Test suite crashed: {e}")
        return 1

if __name__ == "__main__":
    exit(main())
