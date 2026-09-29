# minikv Python SDK (preview)

Notebook-first SDK for data science and engineering workflows.

## Install

```bash
pip install -r sdk/python/requirements.txt
```

## Quick Start

```python
from sdk.python.minikv_client import MinikvClient

client = MinikvClient()

# 1) Upsert vectors
client.vector_upsert("doc-1", [0.1, 0.2, 0.3], {"label": "anomaly"})
client.vector_upsert("doc-2", [0.11, 0.19, 0.29], {"label": "normal"})

# 2) Query nearest neighbors
result = client.vector_query([0.1, 0.2, 0.3], top_k=2)
print(result)

# 3) Query time-series and convert for analysis
ts = client.timeseries_query({"metric": "cpu.usage"})

# Optional dataframe conversion helpers
from sdk.python.minikv_client import to_pandas_points
print(to_pandas_points(ts).head())
```

## Stream Change Events

```python
for event in client.watch_sse():
    print(event)
```

## Covered Endpoints

- /ts/write
- /ts/query
- /vector/upsert
- /vector/query
- /range
- /batch
- /watch/sse
- /metrics
- /admin/backup, /admin/backups and /admin/restore: they answer 501 in minikv 2.0.1, where backups and restores are not implemented
