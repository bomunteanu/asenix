#!/usr/bin/env python3
"""
synthesis_agent.py — Autonomous synthesis agent for Asenix.

Listens for synthesis_needed SSE events, retrieves the relevant cluster of
atoms via query_cluster, calls the Claude API to generate a synthesis, and
publishes the result back to Asenix as a synthesis atom.

Usage:
    ANTHROPIC_API_KEY=sk-... python demo/synthesis_agent.py

Optional environment variables:
    ASENIX_URL       — server base URL (default: http://localhost:3000)
    SYNTHESIS_MODEL  — Claude model to use (default: claude-haiku-4-5-20251001)
    AGENT_NAME       — agent name to register as (default: synthesis-agent-1)

Requirements:
    pip install requests anthropic
"""

import os
import sys
import json
import time
import logging
from typing import Iterator

import requests

logging.basicConfig(
    level=logging.INFO,
    format='[%(asctime)s] %(levelname)s %(message)s',
    datefmt='%H:%M:%S',
)
log = logging.getLogger(__name__)

ASENIX_URL = os.environ.get('ASENIX_URL', 'http://localhost:3000')
ANTHROPIC_API_KEY = os.environ.get('ANTHROPIC_API_KEY', '')
SYNTHESIS_MODEL = os.environ.get('SYNTHESIS_MODEL', 'claude-haiku-4-5-20251001')
AGENT_NAME = os.environ.get('AGENT_NAME', 'synthesis-agent-1')


# ── Asenix JSON-RPC client ────────────────────────────────────────────────────

class AsenixClient:
    def __init__(self, base_url: str):
        self.rpc_url = f'{base_url}/rpc'
        self.agent_id: str | None = None
        self.api_token: str | None = None
        self._req_id = 0

    def _rpc(self, method: str, params: dict) -> dict:
        self._req_id += 1
        resp = requests.post(self.rpc_url, json={
            'jsonrpc': '2.0',
            'id': self._req_id,
            'method': method,
            'params': params,
        }, timeout=30)
        resp.raise_for_status()
        data = resp.json()
        if data.get('error'):
            raise RuntimeError(f'RPC error [{method}]: {data["error"]}')
        return data.get('result', {})

    def register(self, agent_name: str) -> None:
        result = self._rpc('register_agent_simple', {'agent_name': agent_name})
        self.agent_id = result['agent_id']
        self.api_token = result['api_token']
        log.info('Registered as %s: agent_id=%s', agent_name, self.agent_id)

    def query_cluster(self, vector: list[float], radius: float = 0.3,
                      limit: int = 20) -> list[dict]:
        result = self._rpc('query_cluster', {
            'agent_id': self.agent_id,
            'api_token': self.api_token,
            'vector': vector,
            'radius': radius,
            'limit': limit,
        })
        return result.get('atoms', [])

    def publish_synthesis(self, domain: str, statement: str,
                          parent_ids: list[str]) -> str:
        result = self._rpc('publish_atoms', {
            'agent_id': self.agent_id,
            'api_token': self.api_token,
            'atoms': [{
                'atom_type': 'synthesis',
                'domain': domain,
                'statement': statement,
                'provenance': {
                    'parent_ids': parent_ids,
                    'method_description': (
                        f'Synthesised by {AGENT_NAME} using {SYNTHESIS_MODEL}'
                    ),
                },
            }],
        })
        published = result.get('published_atoms', [])
        return published[0] if published else ''


# ── SSE listener ──────────────────────────────────────────────────────────────

def sse_events(base_url: str, types: list[str]) -> Iterator[dict]:
    """Yield parsed SSE event data dicts, reconnecting on connection drop."""
    url = f'{base_url}/events?types={",".join(types)}'
    while True:
        try:
            with requests.get(url, stream=True, timeout=None) as resp:
                resp.raise_for_status()
                log.info('SSE connected to %s', url)
                current_event_type: str | None = None
                for raw_line in resp.iter_lines(decode_unicode=True):
                    if raw_line.startswith('event:'):
                        current_event_type = raw_line[6:].strip()
                    elif raw_line.startswith('data:'):
                        data_str = raw_line[5:].strip()
                        try:
                            payload = json.loads(data_str)
                            if current_event_type:
                                payload['_event_type'] = current_event_type
                            yield payload
                        except json.JSONDecodeError:
                            pass
                        current_event_type = None
        except Exception as exc:
            log.warning('SSE connection lost: %s. Reconnecting in 5s…', exc)
            time.sleep(5)


# ── Claude synthesis ───────────────────────────────────────────────────────────

def synthesise_with_claude(atoms: list[dict], domain: str) -> str:
    try:
        import anthropic
    except ImportError:
        log.error('anthropic package not installed: pip install anthropic')
        raise

    if not ANTHROPIC_API_KEY:
        raise ValueError('ANTHROPIC_API_KEY environment variable not set')

    client = anthropic.Anthropic(api_key=ANTHROPIC_API_KEY)

    summaries = '\n'.join(
        f"- [{a.get('atom_type', 'unknown')}] {a.get('statement', '')}"
        for a in atoms[:20]
    )
    prompt = (
        f'You are a research synthesiser for domain: {domain}\n\n'
        f'Synthesise the following research atoms into a single concise '
        f'summary (2-4 sentences) that captures the collective finding, '
        f'key tensions, and open questions:\n\n{summaries}'
    )

    message = client.messages.create(
        model=SYNTHESIS_MODEL,
        max_tokens=512,
        messages=[{'role': 'user', 'content': prompt}],
    )
    return message.content[0].text.strip()


# ── Main loop ─────────────────────────────────────────────────────────────────

def main() -> None:
    if not ANTHROPIC_API_KEY:
        log.error('ANTHROPIC_API_KEY is not set. Export it before running.')
        sys.exit(1)

    client = AsenixClient(ASENIX_URL)
    client.register(AGENT_NAME)

    log.info('Listening for synthesis_needed events…')
    for event in sse_events(ASENIX_URL, ['synthesis_needed']):
        cluster_center = event.get('cluster_center')
        atom_count = event.get('atom_count', 0)
        domain = event.get('domain', 'unknown')

        log.info('synthesis_needed: domain=%s, atom_count=%d', domain, atom_count)

        if not cluster_center:
            log.warning('No cluster_center in event, skipping')
            continue

        atoms = client.query_cluster(cluster_center, radius=0.3, limit=20)
        if not atoms:
            log.info('No atoms found in cluster, skipping')
            continue

        log.info('Retrieved %d atoms for synthesis', len(atoms))

        try:
            synthesis_text = synthesise_with_claude(atoms, domain)
        except Exception as exc:
            log.error('Claude synthesis failed: %s', exc)
            continue

        parent_ids = [a['atom_id'] for a in atoms if 'atom_id' in a]
        atom_id = client.publish_synthesis(domain, synthesis_text, parent_ids)
        if atom_id:
            log.info('Published synthesis atom: %s', atom_id)
        else:
            log.warning('publish_synthesis returned no atom_id')


if __name__ == '__main__':
    main()
