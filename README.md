# TaskMaster

A Rust-based task management server that creates reversible blockchain transactions on the Quantus Network. TaskMaster periodically selects candidates, creates tasks with reversible transactions, and provides an HTTP API for task completion.

## Features

- 🔄 **Reversible Transactions**: Creates blockchain transactions with configurable reversal periods
- 🎯 **Task Management**: Randomly selects candidates and creates tasks with unique URLs
- 📊 **CSV Persistence**: Stores task state in CSV format for easy monitoring and recovery
- 🌐 **HTTP API**: RESTful endpoints for task completion and status monitoring
- ⏰ **Automated Reversal**: Background service monitors and reverses uncompleted tasks
- 🔗 **GraphQL Integration**: Fetches candidate lists from external GraphQL endpoints
- 📈 **Real-time Monitoring**: Status endpoints for health checks and task statistics

## Architecture

### Core Components

1. **Task Generator**: Periodically fetches candidates and creates random task assignments
2. **Transaction Manager**: Handles blockchain interactions using the quantus-cli SDK
3. **CSV Persistence**: Manages task data storage and state tracking
4. **HTTP Server**: Provides API endpoints for task completion and monitoring
5. **Reverser Service**: Background monitor for automatic transaction reversals
6. **Configuration Manager**: Handles settings and environment variables

### Data Flow

1. Candidates are fetched from a GraphQL endpoint
2. Random taskees are selected and tasks are created with QUAN and USDC amounts
3. Reversible transactions are sent to the Quantus blockchain
4. Tasks are stored in CSV with pending status
5. HTTP API allows tasks to be marked as completed
6. Reverser service monitors for uncompleted tasks approaching expiration
7. Uncompleted tasks are automatically reversed

## Installation

### Prerequisites

- Rust 1.75+ (with cargo)
- Access to a Quantus Network node
- GraphQL endpoint with candidate addresses

### Building from Source

```bash
git clone <repository-url>
cd task-master
cargo build --release
```

The binary will be available at `target/release/task-master`.

## Configuration

TaskMaster uses a TOML configuration file located at `config/default.toml`. You can also override settings using environment variables with the `TASKMASTER_` prefix.

### Configuration File

```toml
[server]
host = "127.0.0.1"
port = 3000

[blockchain]
node_url = "ws://127.0.0.1:9944"
wallet_name = "task_master_wallet"
wallet_password = "secure_password_change_me"
reversal_period_hours = 12

[candidates]
graphql_url = "http://localhost:4000/graphql"
refresh_interval_minutes = 30

[task_generation]
generation_interval_minutes = 60
taskees_per_round = 5

[reverser]
early_reversal_minutes = 2
check_interval_seconds = 30

[data]
csv_file_path = "tasks.csv"

[logging]
level = "info"
```

### Environment Variables

Override any configuration setting using environment variables:

```bash
export TASKMASTER_BLOCKCHAIN__NODE_URL="ws://your-node:9944"
export TASKMASTER_BLOCKCHAIN__WALLET_PASSWORD="your-secure-password"
export TASKMASTER_CANDIDATES__GRAPHQL_URL="https://your-graphql-endpoint.com/graphql"
```

## Usage

### Starting the Server

```bash
# Using default configuration
./task-master

# With custom configuration file
./task-master --config /path/to/config.toml

# With command line overrides
./task-master --wallet-name my_wallet --node-url ws://remote-node:9944

# Run once for testing (no continuous operation)
./task-master --run-once
```

### Command Line Options

```
Options:
  -c, --config <CONFIG>              Configuration file path [default: config/default.toml]
      --wallet-name <WALLET_NAME>    Wallet name override
      --wallet-password <PASSWORD>   Wallet password override
      --node-url <NODE_URL>          Node URL override
      --run-once                     Run once and exit (for testing)
  -h, --help                         Print help
```

## API Endpoints

### Task Completion

Complete a task by providing its task URL:

```bash
curl -X POST http://localhost:3000/complete \
  -H "Content-Type: application/json" \
  -d '{"task_url": "123456789012"}'
```

**Response:**
```json
{
  "success": true,
  "message": "Task completed successfully",
  "task_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### Health Check

```bash
curl http://localhost:3000/health
```

**Response:**
```json
{
  "healthy": true,
  "service": "TaskMaster",
  "version": "0.1.0",
  "timestamp": "2024-01-01T12:00:00Z"
}
```

### Status Information

```bash
curl http://localhost:3000/status
```

**Response:**
```json
{
  "status": "running",
  "total_tasks": 150,
  "pending_tasks": 23,
  "completed_tasks": 89,
  "reversed_tasks": 35,
  "failed_tasks": 3
}
```

### List All Tasks

```bash
curl http://localhost:3000/tasks
```

### Get Specific Task

```bash
curl http://localhost:3000/tasks/550e8400-e29b-41d4-a716-446655440000
```

## Task Data Format

Tasks are stored in CSV format with the following schema:

| Field | Type | Description |
|-------|------|-------------|
| task_id | UUID | Unique task identifier |
| quan_address | String | Quantus address (starts with 'qz') |
| quan_amount | Number | Quantus transaction amount (1000-9999) |
| usdc_amount | Number | USDC reward amount for task completion (1-25) |
| send_time | Timestamp | When transaction was sent |
| end_time | Timestamp | When transaction expires |
| task_url | String | 12-digit random task identifier |
| eth_address | String | Ethereum address (currently unused) |
| status | String | pending, completed, reversed, or failed |
| tx_hash | String | Blockchain transaction hash |

### Task States

- **pending**: Transaction sent, waiting for completion
- **completed**: Task marked as completed via API
- **reversed**: Transaction was reversed due to timeout
- **failed**: Transaction failed to send

## GraphQL Integration

TaskMaster expects a GraphQL endpoint that provides candidate addresses. The query format should be:

```graphql
{
  candidates
}
```

**Expected Response:**
```json
{
  "data": {
    "candidates": [
      "qzkeicNBtW2AG2E7USjDcLzAL8d9WxTZnV2cbtXoDzWxzpHC2",
      "qz7V8J2M3K4L5N6P7Q8R9S0T1U2V3W4X5Y6Z7A8B9C0D1E2F3",
      ...
    ]
  }
}
```

## Monitoring and Logging

### Log Levels

Set the log level in configuration or via environment variable:

```bash
export TASKMASTER_LOGGING__LEVEL=debug
```

Available levels: `error`, `warn`, `info`, `debug`, `trace`

### Key Metrics to Monitor

- **Task Generation Rate**: Tasks created per hour
- **Transaction Success Rate**: Percentage of successful blockchain transactions
- **Reversal Rate**: Percentage of tasks that get reversed
- **Average QUAN Amount**: Monitor transaction amounts (1000-9999)
- **Average USDC Reward**: Monitor reward amounts (1-25)
- **API Response Time**: HTTP endpoint performance
- **Wallet Balance**: Ensure sufficient funds for transactions

### Log Examples

```
2024-01-01T12:00:00Z INFO  🚀 Starting TaskMaster v0.1.0
2024-01-01T12:00:01Z INFO  ✅ Connected to: Quantus Node - Spec: quantus, Version: 100
2024-01-01T12:00:02Z INFO  Wallet address: qzkeicNBtW2AG2E7USjDcLzAL8d9WxTZnV2cbtXoDzWxzpHC2
2024-01-01T12:00:03Z INFO  Loaded 150 candidates
2024-01-01T12:05:00Z INFO  Generated 5 new tasks
2024-01-01T12:05:01Z INFO  Successfully processed 5 transactions
2024-01-01T12:07:30Z INFO  Task 550e8400-e29b-41d4-a716-446655440000 marked as completed
2024-01-01T12:30:00Z INFO  Found 2 tasks ready for reversal
2024-01-01T12:30:01Z INFO  Reversing task abc123 (quan_address: qz..., quan_amount: 2500, usdc_amount: 15, tx: 0x...)
```

## Development

### Running Tests

```bash
cargo test
```

### Running with Debug Logging

```bash
TASKMASTER_LOGGING__LEVEL=debug ./task-master
```

### Database Schema Evolution

The CSV format is designed to be backward compatible. New fields can be added without breaking existing data.

## Security Considerations

### Wallet Security

- Use strong passwords for wallet encryption
- Store wallet passwords securely (environment variables, key management systems)
- The quantus-cli uses quantum-safe encryption for wallet storage
- Private keys are never stored in plain text

### Network Security

- Use TLS for GraphQL endpoint connections in production
- Secure the HTTP API with authentication if exposed publicly
- Monitor for unusual transaction patterns

### Operational Security

- Regularly monitor wallet balance
- Set up alerts for high reversal rates
- Review task completion patterns for anomalies

## Troubleshooting

### Common Issues

**Connection to Quantus node fails:**
```
Error: Node health check failed: NetworkError("Connection refused")
```
- Verify the node URL is correct
- Ensure the Quantus node is running and accessible
- Check firewall settings

**Wallet creation fails:**
```
Error: Wallet error: AlreadyExists
```
- Wallet with the same name already exists
- Use a different wallet name or load the existing wallet

**GraphQL endpoint unreachable:**
```
Error: Failed to refresh candidates: Http(reqwest::Error { ... })
```
- Verify the GraphQL URL is correct
- Check network connectivity
- Ensure the GraphQL endpoint is running

**CSV file permissions:**
```
Error: CSV error: Io(Os { code: 13, kind: PermissionDenied, message: "Permission denied" })
```
- Ensure write permissions for the CSV file directory
- Check disk space availability

### Performance Tuning

- Adjust `check_interval_seconds` for reverser service based on load
- Tune `generation_interval_minutes` based on candidate pool size
- Monitor memory usage with large CSV files

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes and add tests
4. Commit your changes (`git commit -m 'Add amazing feature'`)
5. Push to the branch (`git push origin feature/amazing-feature`)
6. Open a Pull Request

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Dependencies

- [quantus-cli](https://github.com/Quantus-Network/quantus-cli) - Quantus blockchain interaction
- [tokio](https://tokio.rs/) - Async runtime
- [axum](https://github.com/tokio-rs/axum) - HTTP server framework
- [serde](https://serde.rs/) - Serialization framework
- [csv](https://github.com/BurntSushi/rust-csv) - CSV processing
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client
- [chrono](https://github.com/chronotope/chrono) - Date/time handling
- [uuid](https://github.com/uuid-rs/uuid) - UUID generation
- [tracing](https://github.com/tokio-rs/tracing) - Logging framework