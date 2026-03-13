# Deployment Guide

## Overview

This guide covers deploying Mote in various environments, from development to production.

## Deployment Options

### 1. Local Development
### 2. Docker Container
### 3. Kubernetes Cluster
### 4. Cloud Services

## Prerequisites

### System Requirements
- **CPU**: 2+ cores (4+ recommended for production)
- **Memory**: 4GB+ RAM (8GB+ recommended for production)
- **Storage**: 20GB+ SSD (100GB+ recommended for production)
- **Network**: Stable internet connection for embedding services

### External Dependencies
- **PostgreSQL 15+** with pgvector extension
- **Embedding Service** (OpenAI API, local embedding model, etc.)
- **Optional**: Redis for caching
- **Optional**: Object storage for large files

## Configuration Management

### Environment Variables
```bash
# Database
DATABASE_URL=postgresql://user:password@localhost:5432/mote

# Logging
RUST_LOG=info
RUST_LOG_FORMAT=json

# Security
JWT_SECRET=your-secret-key
RATE_LIMIT_SECRET=your-rate-limit-secret

# External Services
EMBEDDING_SERVICE_URL=http://localhost:8080/embed
EMBEDDING_API_KEY=your-api-key

# Monitoring
PROMETHEUS_PORT=9090
METRICS_ENABLED=true
```

### Configuration Files
```toml
# config.toml
[hub]
name = "production-mote"
domain = "research"
listen_address = "0.0.0.0:3000"
embedding_endpoint = "${EMBEDDING_SERVICE_URL}"
embedding_model = "text-embedding-ada-002"
embedding_dimension = 1536

[trust]
reliability_threshold = 0.3
max_atoms_per_hour = 10000

[workers]
embedding_pool_size = 32
decay_interval_minutes = 60
claim_ttl_hours = 24
```

## Docker Deployment

### Dockerfile
```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

# Build dependencies
RUN cargo build --release
RUN cargo install --path .

FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    postgresql-client \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 mote

WORKDIR /app
COPY --from=builder /usr/local/cargo/bin/mote /usr/local/bin/
COPY --from=builder /app/migrations ./migrations
COPY config.example.toml ./config.toml

USER mote

EXPOSE 3000

CMD ["mote", "--config", "config.toml"]
```

### Docker Compose (Development)
```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: pgvector/pgvector:pg15
    environment:
      POSTGRES_DB: mote
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: password
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 30s
      timeout: 10s
      retries: 3

  mote:
    build: .
    ports:
      - "3000:3000"
    environment:
      DATABASE_URL: postgresql://postgres:password@postgres:5432/mote
      RUST_LOG: info
    depends_on:
      postgres:
        condition: service_healthy
    volumes:
      - ./config.toml:/app/config.toml
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

### Docker Compose (Production)
```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  postgres:
    image: pgvector/pgvector:pg15
    environment:
      POSTGRES_DB: mote
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d
    restart: always
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER}"]
      interval: 30s
      timeout: 10s
      retries: 3

  mote:
    image: your-registry/mote:latest
    ports:
      - "3000:3000"
    environment:
      DATABASE_URL: postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/mote
      RUST_LOG: info
      EMBEDDING_SERVICE_URL: ${EMBEDDING_SERVICE_URL}
      EMBEDDING_API_KEY: ${EMBEDDING_API_KEY}
    depends_on:
      postgres:
        condition: service_healthy
    volumes:
      - ./config.toml:/app/config.toml
      - ./logs:/app/logs
    restart: always
    deploy:
      replicas: 2
      resources:
        limits:
          cpus: '1.0'
          memory: 2G
        reservations:
          cpus: '0.5'
          memory: 1G

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
      - ./ssl:/etc/nginx/ssl
    depends_on:
      - mote
    restart: always

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus_data:/prometheus
    restart: always

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3001:3000"
    environment:
      GF_SECURITY_ADMIN_PASSWORD: ${GRAFANA_PASSWORD}
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    restart: always

volumes:
  postgres_data:
  prometheus_data:
  grafana_data:
```

## Kubernetes Deployment

### Namespace
```yaml
# namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: mote
```

### ConfigMap
```yaml
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mote-config
  namespace: mote
data:
  config.toml: |
    [hub]
    name = "production-mote"
    domain = "research"
    listen_address = "0.0.0.0:3000"
    embedding_endpoint = "http://embedding-service:8080/embed"
    embedding_model = "text-embedding-ada-002"
    embedding_dimension = 1536

    [trust]
    reliability_threshold = 0.3
    max_atoms_per_hour = 10000

    [workers]
    embedding_pool_size = 32
    decay_interval_minutes = 60
```

### Secret
```yaml
# secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: mote-secrets
  namespace: mote
type: Opaque
data:
  database-url: cG9zdGdyZXNxbDovL3VzZXI6cGFzc3dvcmRAcG9zdGdyZXM6NTQzMi9tb3Rl # base64 encoded
  embedding-api-key: eW91ci1hcGkta2V5 # base64 encoded
```

### Deployment
```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mote
  namespace: mote
spec:
  replicas: 3
  selector:
    matchLabels:
      app: mote
  template:
    metadata:
      labels:
        app: mote
    spec:
      containers:
      - name: mote
        image: your-registry/mote:latest
        ports:
        - containerPort: 3000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: mote-secrets
              key: database-url
        - name: EMBEDDING_API_KEY
          valueFrom:
            secretKeyRef:
              name: mote-secrets
              key: embedding-api-key
        - name: RUST_LOG
          value: "info"
        volumeMounts:
        - name: config
          mountPath: /app/config.toml
          subPath: config.toml
        resources:
          requests:
            memory: "1Gi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "1000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: mote-config
```

### Service
```yaml
# service.yaml
apiVersion: v1
kind: Service
metadata:
  name: mote-service
  namespace: mote
spec:
  selector:
    app: mote
  ports:
  - protocol: TCP
    port: 80
    targetPort: 3000
  type: ClusterIP
```

### Ingress
```yaml
# ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: mote-ingress
  namespace: mote
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  tls:
  - hosts:
    - mote.example.com
    secretName: mote-tls
  rules:
  - host: mote.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: mote-service
            port:
              number: 80
```

### StatefulSet for PostgreSQL
```yaml
# postgres.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
  namespace: mote
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
      - name: postgres
        image: pgvector/pgvector:pg15
        env:
        - name: POSTGRES_DB
          value: mote
        - name: POSTGRES_USER
          value: postgres
        - name: POSTGRES_PASSWORD
          value: password
        ports:
        - containerPort: 5432
        volumeMounts:
        - name: postgres-storage
          mountPath: /var/lib/postgresql/data
        - name: migrations
          mountPath: /docker-entrypoint-initdb.d
      volumes:
      - name: migrations
        configMap:
          name: postgres-migrations
  volumeClaimTemplates:
  - metadata:
      name: postgres-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 100Gi
```

## Cloud Deployment

### AWS ECS

#### Task Definition
```json
{
  "family": "mote",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "1024",
  "memory": "2048",
  "executionRoleArn": "arn:aws:iam::account:role/ecsTaskExecutionRole",
  "taskRoleArn": "arn:aws:iam::account:role/ecsTaskRole",
  "containerDefinitions": [
    {
      "name": "mote",
      "image": "your-account.dkr.ecr.region.amazonaws.com/mote:latest",
      "portMappings": [
        {
          "containerPort": 3000,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {
          "name": "DATABASE_URL",
          "value": "postgresql://user:pass@rds-endpoint:5432/mote"
        },
        {
          "name": "RUST_LOG",
          "value": "info"
        }
      ],
      "secrets": [
        {
          "name": "EMBEDDING_API_KEY",
          "valueFrom": "arn:aws:secretsmanager:region:account:secret:mote/embedding-key"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/mote",
          "awslogs-region": "us-west-2",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:3000/health || exit 1"],
        "interval": 30,
        "timeout": 5,
        "retries": 3
      }
    }
  ]
}
```

### Google Cloud Run

#### Deployment Script
```bash
#!/bin/bash

# Build and push to Google Container Registry
gcloud builds submit --tag gcr.io/PROJECT-ID/mote

# Deploy to Cloud Run
gcloud run deploy mote \
  --image gcr.io/PROJECT-ID/mote \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --set-env-vars DATABASE_URL=$DATABASE_URL \
  --set-env-vars RUST_LOG=info \
  --set-secrets EMBEDDING_API_KEY=embedding-key:latest \
  --memory 2Gi \
  --cpu 1 \
  --min-instances 1 \
  --max-instances 10
```

## Monitoring and Observability

### Prometheus Configuration
```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'mote'
    static_configs:
      - targets: ['mote:3000']
    metrics_path: /metrics
    scrape_interval: 10s

  - job_name: 'postgres'
    static_configs:
      - targets: ['postgres:5432']
```

### Grafana Dashboard
```json
{
  "dashboard": {
    "title": "Mote Monitoring",
    "panels": [
      {
        "title": "Request Rate",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(http_requests_total[5m])",
            "legendFormat": "{{method}} {{status}}"
          }
        ]
      },
      {
        "title": "Database Connections",
        "type": "graph", 
        "targets": [
          {
            "expr": "pg_stat_database_numbackends",
            "legendFormat": "Active Connections"
          }
        ]
      }
    ]
  }
}
```

## Security Considerations

### Network Security
```yaml
# Network policy example
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: mote-network-policy
  namespace: mote
spec:
  podSelector:
    matchLabels:
      app: mote
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 3000
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: postgres
    ports:
    - protocol: TCP
      port: 5432
```

### Secrets Management
```bash
# Kubernetes secrets
kubectl create secret generic mote-secrets \
  --from-literal=database-url="postgresql://..." \
  --from-literal=embedding-api-key="..." \
  -n mote

# AWS Secrets Manager
aws secretsmanager create-secret \
  --name mote/production \
  --secret-string '{"DATABASE_URL":"...","EMBEDDING_API_KEY":"..."}'
```

## Backup and Recovery

### Database Backup
```bash
# Automated backup script
#!/bin/bash

BACKUP_DIR="/backups/mote"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/mote_backup_$DATE.sql"

# Create backup directory
mkdir -p $BACKUP_DIR

# Perform backup
pg_dump -h postgres -U postgres -d mote > $BACKUP_FILE

# Compress backup
gzip $BACKUP_FILE

# Upload to S3 (or other storage)
aws s3 cp $BACKUP_FILE.gz s3://mote-backups/

# Clean up old backups (keep last 30 days)
find $BACKUP_DIR -name "*.sql.gz" -mtime +30 -delete
```

### Kubernetes Backup
```yaml
# CronJob for backups
apiVersion: batch/v1
kind: CronJob
metadata:
  name: mote-backup
  namespace: mote
spec:
  schedule: "0 2 * * *"  # Daily at 2 AM
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: postgres:15
            command:
            - /bin/bash
            - -c
            - |
              pg_dump -h postgres -U postgres -d mote | gzip > /backup/mote_$(date +%Y%m%d_%H%M%S).sql.gz
            env:
            - name: PGPASSWORD
              value: "password"
            volumeMounts:
            - name: backup-storage
              mountPath: /backup
          volumes:
          - name: backup-storage
            persistentVolumeClaim:
              claimName: backup-pvc
          restartPolicy: OnFailure
```

## Scaling Strategies

### Horizontal Scaling
```yaml
# Horizontal Pod Autoscaler
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: mote-hpa
  namespace: mote
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: mote
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

### Database Scaling
```bash
# Connection pooling with PgBouncer
docker run -d \
  --name pgbouncer \
  -p 6432:6432 \
  -e DATABASES_HOST=postgres \
  -e DATABASES_PORT=5432 \
  -e DATABASES_USER=postgres \
  -e DATABASES_PASSWORD=password \
  -e DATABASES_DBNAME=mote \
  -e POOL_MODE=transaction \
  -e MAX_CLIENT_CONN=100 \
  -e DEFAULT_POOL_SIZE=20 \
  pgbouncer/pgbouncer:latest
```

## Troubleshooting

### Common Issues

#### 1. Database Connection Failures
```bash
# Check PostgreSQL logs
kubectl logs -n mote postgres-0

# Test connection from application pod
kubectl exec -it mote-xxx -- psql -h postgres -U postgres -d mote -c "SELECT 1;"

# Check network policies
kubectl get networkpolicy -n mote
```

#### 2. High Memory Usage
```bash
# Check memory usage
kubectl top pods -n mote

# Check for memory leaks
kubectl exec -it mote-xxx -- ps aux

# Restart deployment
kubectl rollout restart deployment/mote -n mote
```

#### 3. Slow Performance
```bash
# Check database queries
kubectl exec -it postgres-0 -- psql -U postgres -d mote -c "
  SELECT query, mean_time, calls 
  FROM pg_stat_statements 
  ORDER BY mean_time DESC 
  LIMIT 10;"

# Check resource usage
kubectl describe pod mote-xxx -n mote
```

### Debug Commands
```bash
# Check pod status
kubectl get pods -n mote -o wide

# View logs
kubectl logs -f deployment/mote -n mote

# Exec into pod
kubectl exec -it deployment/mote -n mote -- /bin/bash

# Check events
kubectl get events -n mote --sort-by=.metadata.creationTimestamp
```

## Maintenance

### Rolling Updates
```bash
# Update deployment with new image
kubectl set image deployment/mote mote=your-registry/mote:v2.0.0 -n mote

# Monitor rollout status
kubectl rollout status deployment/mote -n mote

# Rollback if needed
kubectl rollout undo deployment/mote -n mote
```

### Health Monitoring
```bash
# Check health endpoints
curl http://mote.example.com/health

# Monitor metrics
curl http://mote.example.com/metrics

# Check database health
kubectl exec -it postgres-0 -- pg_isready -U postgres
```

This deployment guide provides comprehensive instructions for deploying Mote in various environments, from local development to production Kubernetes clusters.
