#!/bin/bash

# Integration Test Database Setup Script
# This script sets up the test database for running integration tests
# Enhanced for Phase 6 testing with pgvector support

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🐳 Setting up integration test database for Phase 6...${NC}"

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running. Please start Docker first.${NC}"
    exit 1
fi

# Check if docker-compose.test.yml exists
if [ ! -f "docker-compose.test.yml" ]; then
    echo -e "${RED}❌ docker-compose.test.yml not found${NC}"
    exit 1
fi

# Clean up any existing test database
echo -e "${YELLOW}🧹 Cleaning up any existing test database...${NC}"
docker-compose -f docker-compose.test.yml down -v 2>/dev/null || true

# Start test database
echo -e "${BLUE}📦 Starting test database container with pgvector...${NC}"
docker-compose -f docker-compose.test.yml up -d postgres

# Wait for database to be ready with better timeout
echo -e "${YELLOW}⏳ Waiting for database to be ready...${NC}"
max_attempts=60
attempt=1
while [ $attempt -le $max_attempts ]; do
    if docker-compose -f docker-compose.test.yml exec -T postgres pg_isready -U postgres -d mote_test > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Database is ready!${NC}"
        break
    fi
    
    if [ $attempt -eq $max_attempts ]; then
        echo -e "${RED}❌ Database failed to start within ${max_attempts} seconds${NC}"
        echo -e "${RED}📋 Database logs:${NC}"
        docker-compose -f docker-compose.test.yml logs postgres
        exit 1
    fi
    
    echo -e "${YELLOW}⏳ Attempt $attempt/${max_attempts}...${NC}"
    sleep 1
    attempt=$((attempt + 1))
done

# Verify pgvector extension
echo -e "${BLUE}🔍 Verifying pgvector extension...${NC}"
if ! docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -c "SELECT 1 FROM pg_extension WHERE extname = 'vector';" | grep -q 1; then
    echo -e "${RED}❌ pgvector extension not found. Please ensure the Docker image includes pgvector.${NC}"
    exit 1
else
    echo -e "${GREEN}✅ pgvector extension is available${NC}"
fi

# Run database migrations using SQLx CLI
echo -e "${BLUE}🗄️ Running database migrations...${NC}"

# Check if sqlx-cli is installed
if ! command -v sqlx &> /dev/null; then
    echo -e "${YELLOW}⚠️ sqlx-cli not found, installing via cargo...${NC}"
    cargo install sqlx-cli --no-default-features --features postgres,rustls,sqlite
fi

# Set database URL for sqlx
export DATABASE_URL="postgresql://postgres:password@localhost:5432/mote_test"

# Run migrations
if sqlx migrate run --source migrations > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Database migrations completed successfully${NC}"
else
    echo -e "${YELLOW}⚠️ Migration failed, trying manual approach...${NC}"
    
    # Fallback to manual migration
    if [ -f "migrations/001_initial_schema.sql" ]; then
        docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -f /docker-entrypoint-initdb.d/001_initial_schema.sql > /dev/null 2>&1
        echo -e "${GREEN}✅ Manual migration completed${NC}"
    else
        echo -e "${RED}❌ Migration file not found${NC}"
        exit 1
    fi
fi

# Verify database schema
echo -e "${BLUE}🔍 Verifying database schema...${NC}"
required_tables=("atoms" "agents" "claims" "edges")
for table in "${required_tables[@]}"; do
    if docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -c "SELECT 1 FROM information_schema.tables WHERE table_name = '$table';" | grep -q 1; then
        echo -e "${GREEN}✅ Table '$table' exists${NC}"
    else
        echo -e "${RED}❌ Table '$table' missing${NC}"
        exit 1
    fi
done

# Test Phase 6 specific features
echo -e "${BLUE}🧪 Testing Phase 6 database features...${NC}"

# Test vector operations
if docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -c "SELECT '[1,2,3]'::vector;" > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Vector operations working${NC}"
else
    echo -e "${RED}❌ Vector operations failed${NC}"
    exit 1
fi

# Test HNSW index creation
if docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -c "CREATE TABLE test_vector (id serial PRIMARY KEY, embedding vector(1536)); CREATE INDEX ON test_vector USING hnsw (embedding vector_cosine_ops);" > /dev/null 2>&1; then
    echo -e "${GREEN}✅ HNSW index creation working${NC}"
    docker-compose -f docker-compose.test.yml exec -T postgres psql -U postgres -d mote_test -c "DROP TABLE test_vector;" > /dev/null 2>&1
else
    echo -e "${YELLOW}⚠️ HNSW index creation failed (may not be critical)${NC}"
fi

echo -e "${GREEN}🎯 Test database setup complete!${NC}"
echo ""
echo -e "${BLUE}📋 To run integration tests:${NC}"
echo -e "   ${YELLOW}export DATABASE_URL=\"postgresql://postgres:password@localhost:5432/mote_test\"${NC}"
echo -e "   ${YELLOW}cargo test --test integration${NC}"
echo ""
echo -e "${BLUE}🧪 To run specific Phase 6 tests:${NC}"
echo -e "   ${YELLOW}cargo test --test integration -- health_tests${NC}"
echo -e "   ${YELLOW}cargo test --test integration -- agent_registration_tests${NC}"
echo -e "   ${YELLOW}cargo test --test integration -- schema_tests${NC}"
echo ""
echo -e "${BLUE}🧹 To clean up test database:${NC}"
echo -e "   ${YELLOW}docker-compose -f docker-compose.test.yml down -v${NC}"
echo ""
echo -e "${BLUE}📊 To monitor database activity:${NC}"
echo -e "   ${YELLOW}docker-compose -f docker-compose.test.yml logs -f postgres${NC}"
echo ""
echo -e "${GREEN}🚀 Ready for Phase 6 integration testing!${NC}"
