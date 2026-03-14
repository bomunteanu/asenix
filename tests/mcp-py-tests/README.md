# Self-contained: Start/stop Docker automatically
python run_tests.py --self-contained

# Standard: Use existing Docker services  
python run_tests.py

# No-Docker: Skip Docker-dependent tests
python run_tests.py --no-docker

# Help: Show all options
python run_tests.py --help