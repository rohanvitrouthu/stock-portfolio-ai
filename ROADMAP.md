# Roadmap

## Phase 1: Setup

- initialize the repository with `uv`
- standardize Python 3.13 usage
- define dependency and configuration foundations
- establish project documentation and local environment templates

## Phase 2: Data Agent

- ✅ build a market data ingestion layer around `yfinance`
- normalize price history, company metadata, and key financial fields
- add retry, caching, and data validation paths
- ✅ expose data retrieval through a dedicated agent or graph node

## Phase 3: Analyst Agents

- create specialized agents for fundamentals, technicals, macro context, and news synthesis
- ✅ define shared report schema and initial evidence collection rules
- define shared prompts and richer evidence collection rules
- standardize outputs into machine-readable analyst reports
- add tracing and evaluation hooks for agent quality

## Phase 4: Portfolio Manager

- combine analyst outputs into position sizing and allocation proposals
- track watchlists, constraints, and rebalance rules
- support scenario analysis and portfolio-level reasoning
- generate transparent rationale for recommendations

## Phase 5: Supervisor

- orchestrate the full workflow across data, analyst, and portfolio nodes
- manage task routing, failure handling, and execution policies
- add persistent state for multi-step portfolio reviews
- define guardrails for incomplete or conflicting evidence

## Phase 6: UI/CLI

- provide a CLI entrypoint for research and portfolio review flows
- add a lightweight UI for dashboards, reports, and agent interaction
- surface configuration, run history, and audit trails
- prepare the system for deployment and operator workflows
