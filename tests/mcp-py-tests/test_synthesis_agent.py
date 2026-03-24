"""
Unit tests for synthesis_agent.py.
All tests are mock-based — no live server required.
"""

import importlib.util
import json
import os
import sys
import unittest
from unittest.mock import MagicMock, patch

# ── Load synthesis_agent module without running main() ────────────────────────

_AGENT_PATH = os.path.join(
    os.path.dirname(__file__), '..', '..', 'demo', 'synthesis_agent.py'
)
spec = importlib.util.spec_from_file_location('synthesis_agent', _AGENT_PATH)
_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(_module)  # type: ignore[union-attr]

AsenixClient = _module.AsenixClient
synthesise_with_claude = _module.synthesise_with_claude
sse_events = _module.sse_events


# ── AsenixClient.register ─────────────────────────────────────────────────────

class TestAsenixClientRegister(unittest.TestCase):
    @patch('requests.post')
    def test_stores_agent_id_and_token(self, mock_post):
        mock_post.return_value.json.return_value = {
            'result': {'agent_id': 'agent-abc', 'api_token': 'tok-xyz'},
        }
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.register('test-agent')

        self.assertEqual(client.agent_id, 'agent-abc')
        self.assertEqual(client.api_token, 'tok-xyz')

    @patch('requests.post')
    def test_raises_on_rpc_error(self, mock_post):
        mock_post.return_value.json.return_value = {
            'error': {'code': -32600, 'message': 'Invalid request'},
        }
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        with self.assertRaises(RuntimeError):
            client.register('test-agent')


# ── AsenixClient.query_cluster ────────────────────────────────────────────────

class TestAsenixClientQueryCluster(unittest.TestCase):
    @patch('requests.post')
    def test_returns_atoms(self, mock_post):
        mock_post.return_value.json.return_value = {
            'result': {
                'atoms': [
                    {'atom_id': 'a1', 'atom_type': 'finding',
                     'statement': 'Test finding', 'domain': 'ml'},
                ],
                'total': 1,
            }
        }
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.agent_id = 'agent-1'
        atoms = client.query_cluster([0.1, 0.2, 0.3], radius=0.5)

        self.assertEqual(len(atoms), 1)
        self.assertEqual(atoms[0]['atom_id'], 'a1')

    @patch('requests.post')
    def test_sends_correct_params(self, mock_post):
        mock_post.return_value.json.return_value = {'result': {'atoms': []}}
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.query_cluster([0.1, 0.2], radius=0.25, limit=10)

        call_params = mock_post.call_args[1]['json']['params']
        self.assertEqual(call_params['radius'], 0.25)
        self.assertEqual(call_params['limit'], 10)
        self.assertEqual(call_params['vector'], [0.1, 0.2])

    @patch('requests.post')
    def test_returns_empty_list_when_no_atoms(self, mock_post):
        mock_post.return_value.json.return_value = {'result': {}}
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        atoms = client.query_cluster([0.1], radius=0.3)
        self.assertEqual(atoms, [])


# ── AsenixClient.publish_synthesis ───────────────────────────────────────────

class TestAsenixClientPublishSynthesis(unittest.TestCase):
    @patch('requests.post')
    def test_returns_atom_id(self, mock_post):
        mock_post.return_value.json.return_value = {
            'result': {'published_atoms': ['synth-atom-1']}
        }
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.agent_id = 'agent-1'
        client.api_token = 'tok-abc'
        atom_id = client.publish_synthesis('ml', 'Test synthesis', ['a1', 'a2'])

        self.assertEqual(atom_id, 'synth-atom-1')

    @patch('requests.post')
    def test_sends_synthesis_atom_type(self, mock_post):
        mock_post.return_value.json.return_value = {
            'result': {'published_atoms': ['synth-1']}
        }
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.agent_id = 'agent-1'
        client.api_token = 'tok-abc'
        client.publish_synthesis('nlp', 'A synthesis', ['x'])

        call_params = mock_post.call_args[1]['json']['params']
        atom = call_params['atoms'][0]
        self.assertEqual(atom['atom_type'], 'synthesis')
        self.assertEqual(atom['domain'], 'nlp')
        self.assertIn('x', atom['provenance']['parent_ids'])

    @patch('requests.post')
    def test_returns_empty_string_when_no_published_atoms(self, mock_post):
        mock_post.return_value.json.return_value = {'result': {}}
        mock_post.return_value.raise_for_status = lambda: None

        client = AsenixClient('http://localhost:3000')
        client.agent_id = 'a'
        client.api_token = 't'
        atom_id = client.publish_synthesis('ml', 'text', [])
        self.assertEqual(atom_id, '')


# ── synthesise_with_claude ────────────────────────────────────────────────────

def _make_anthropic_mock():
    """Return a mock anthropic module with a stubbed Anthropic client class."""
    mock_anthropic_mod = MagicMock()
    mock_client_instance = MagicMock()
    mock_anthropic_mod.Anthropic.return_value = mock_client_instance
    return mock_anthropic_mod, mock_client_instance


class TestSynthesiseWithClaude(unittest.TestCase):
    def test_calls_claude_and_returns_text(self):
        mock_mod, mock_client = _make_anthropic_mock()
        mock_client.messages.create.return_value.content = [
            MagicMock(text='  Synthesised: finding A supports hypothesis B.  ')
        ]
        atoms = [
            {'atom_type': 'finding', 'statement': 'Finding A'},
            {'atom_type': 'hypothesis', 'statement': 'Hypothesis B'},
        ]
        with patch.dict(sys.modules, {'anthropic': mock_mod}), \
             patch.object(_module, 'ANTHROPIC_API_KEY', 'test-key'):
            result = synthesise_with_claude(atoms, 'ml')

        self.assertEqual(result, 'Synthesised: finding A supports hypothesis B.')
        mock_client.messages.create.assert_called_once()
        call_kwargs = mock_client.messages.create.call_args[1]
        self.assertIn('ml', call_kwargs['messages'][0]['content'])

    def test_raises_without_api_key(self):
        atoms = [{'atom_type': 'finding', 'statement': 'X'}]
        with patch.object(_module, 'ANTHROPIC_API_KEY', ''):
            with self.assertRaises((ValueError, Exception)):
                synthesise_with_claude(atoms, 'ml')

    def test_truncates_to_20_atoms(self):
        mock_mod, mock_client = _make_anthropic_mock()
        mock_client.messages.create.return_value.content = [MagicMock(text='ok')]

        atoms = [{'atom_type': 'finding', 'statement': f'Finding {i}'} for i in range(30)]
        with patch.dict(sys.modules, {'anthropic': mock_mod}), \
             patch.object(_module, 'ANTHROPIC_API_KEY', 'k'):
            synthesise_with_claude(atoms, 'ml')

        prompt = mock_client.messages.create.call_args[1]['messages'][0]['content']
        # Only 20 findings should appear in the prompt
        self.assertEqual(prompt.count('Finding'), 20)


# ── sse_events generator ──────────────────────────────────────────────────────

class TestSseEventsGenerator(unittest.TestCase):
    @patch('requests.get')
    def test_yields_parsed_event(self, mock_get):
        lines = [
            'event: synthesis_needed',
            'data: {"cluster_center":[0.1,0.2],"atom_count":5,"domain":"ml"}',
            '',
        ]
        mock_resp = MagicMock()
        mock_resp.iter_lines.return_value = iter(lines)
        mock_resp.raise_for_status = lambda: None
        mock_resp.__enter__ = lambda s: s
        mock_resp.__exit__ = MagicMock(return_value=False)
        mock_get.return_value = mock_resp

        gen = sse_events('http://localhost:3000', ['synthesis_needed'])
        event = next(gen)

        self.assertEqual(event['domain'], 'ml')
        self.assertEqual(event['atom_count'], 5)
        self.assertEqual(event['cluster_center'], [0.1, 0.2])
        self.assertEqual(event['_event_type'], 'synthesis_needed')

    @patch('requests.get')
    def test_skips_malformed_data(self, mock_get):
        lines = [
            'event: synthesis_needed',
            'data: not-valid-json',
            '',
            'event: synthesis_needed',
            'data: {"atom_count":3,"domain":"nlp","cluster_center":[0.5]}',
            '',
        ]
        mock_resp = MagicMock()
        mock_resp.iter_lines.return_value = iter(lines)
        mock_resp.raise_for_status = lambda: None
        mock_resp.__enter__ = lambda s: s
        mock_resp.__exit__ = MagicMock(return_value=False)
        mock_get.return_value = mock_resp

        gen = sse_events('http://localhost:3000', ['synthesis_needed'])
        event = next(gen)  # should be the second (valid) event
        self.assertEqual(event['domain'], 'nlp')

    @patch('time.sleep')
    @patch('requests.get')
    def test_reconnects_on_connection_error(self, mock_get, mock_sleep):
        # First call raises, second returns a valid event
        good_lines = [
            'event: synthesis_needed',
            'data: {"atom_count":1,"domain":"bio","cluster_center":[0.1]}',
            '',
        ]
        mock_good = MagicMock()
        mock_good.iter_lines.return_value = iter(good_lines)
        mock_good.raise_for_status = lambda: None
        mock_good.__enter__ = lambda s: s
        mock_good.__exit__ = MagicMock(return_value=False)

        mock_get.side_effect = [ConnectionError('refused'), mock_good]

        gen = sse_events('http://localhost:3000', ['synthesis_needed'])
        event = next(gen)
        self.assertEqual(event['domain'], 'bio')
        mock_sleep.assert_called_once_with(5)


if __name__ == '__main__':
    unittest.main()
