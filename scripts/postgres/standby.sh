#!/bin/bash
# PostgreSQL Standby (Secondary) initialization script
# Configures a PostgreSQL instance to replicate from primary

set -e

echo "Starting PostgreSQL standby initialization..."

# Wait for primary to be ready
PRIMARY_HOST=${PRIMARY_HOST:-postgres-primary}
PRIMARY_PORT=${PRIMARY_PORT:-5432}
MAX_RETRIES=30
RETRY_COUNT=0

echo "Waiting for primary PostgreSQL ($PRIMARY_HOST:$PRIMARY_PORT) to be ready..."
until pg_isready -h "$PRIMARY_HOST" -p "$PRIMARY_PORT" -U "$PGUSER" || [ $RETRY_COUNT -eq $MAX_RETRIES ]; do
    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "Retry $RETRY_COUNT/$MAX_RETRIES: Waiting for primary..."
    sleep 2
done

if [ $RETRY_COUNT -eq $MAX_RETRIES ]; then
    echo "ERROR: Primary PostgreSQL did not become ready after $MAX_RETRIES retries"
    exit 1
fi

echo "Primary is ready. Setting up replication slot..."

# Create replication slot on primary
psql -h "$PRIMARY_HOST" -p "$PRIMARY_PORT" -U "$PGUSER" -d postgres -c \
    "SELECT * FROM pg_create_physical_replication_slot('replication_slot_1', true);" 2>/dev/null || true

# Take base backup from primary
echo "Taking base backup from primary..."
rm -rf /var/lib/postgresql/data/*
pg_basebackup \
    -h "$PRIMARY_HOST" \
    -p "$PRIMARY_PORT" \
    -U "$PGUSER" \
    -D /var/lib/postgresql/data \
    -v \
    -P \
    -W \
    -X stream \
    -C \
    -S replication_slot_1

# Create recovery.conf for PostgreSQL 12+
echo "Configuring recovery parameters..."

cat > /var/lib/postgresql/data/recovery.conf <<EOF
primary_conninfo = 'host=$PRIMARY_HOST port=$PRIMARY_PORT user=$PGUSER password=$PGPASSWORD application_name=standby1'
restore_command = 'exit 1'
standby_mode = 'on'
recovery_target_timeline = 'latest'
wal_receiver_status_interval = 10s
hot_standby = on
max_standby_streaming_delay = 300s
wal_receiver_timeout = 60s
wal_retrieve_retry_interval = 5s
EOF

# For PostgreSQL 12+, use recovery.signal
if [ ! -f /var/lib/postgresql/data/postgresql.conf ]; then
    echo "ERROR: postgresql.conf not found"
    exit 1
fi

# Create standby.signal to enable standby mode in PostgreSQL 12+
touch /var/lib/postgresql/data/standby.signal

# Fix permissions
chown postgres:postgres /var/lib/postgresql/data/recovery.conf
chmod 600 /var/lib/postgresql/data/recovery.conf
chown postgres:postgres /var/lib/postgresql/data/standby.signal
chmod 600 /var/lib/postgresql/data/standby.signal

echo "PostgreSQL standby configuration complete."
echo "Starting PostgreSQL..."

# Start PostgreSQL
exec postgres
