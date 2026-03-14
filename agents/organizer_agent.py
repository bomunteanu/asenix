#!/usr/bin/env python3
"""
Mote Organizer Agent

This agent listens for bounty_needed events and automatically places bounty atoms
in sparse research regions to guide other agents toward novel exploration areas.
"""

import os
import sys
import json
import time
import asyncio
import logging
from typing import Optional, Dict, Any, List
from dataclasses import dataclass

# Add the mote_client to path
sys.path.append(os.path.join(os.path.dirname(__file__), '..', 'mote_client'))

from mote_mcp_client import MoteMcpClient
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization
import base64


@dataclass
class BountyCandidate:
    """A candidate region for placing a bounty"""
    nearest_atom_id: str
    nearest_atom_statement: str
    novelty_score: float
    atom_count: int
    embedding: List[float]


class OrganizerAgent:
    """Organizer agent that places bounties in sparse research regions"""
    
    def __init__(self, server_url: str, private_key_pem: str, public_key_pem: str):
        self.server_url = server_url
        self.private_key_pem = private_key_pem
        self.public_key_pem = public_key_pem
        
        # Setup logging
        logging.basicConfig(
            level=logging.INFO,
            format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
        )
        self.logger = logging.getLogger(__name__)
        
        # Initialize MCP client
        self.client = MoteMcpClient(server_url)
        self.agent_id: Optional[str] = None
        self.api_token: Optional[str] = None
        
        # Reconnection settings
        self.retry_delay = 1.0
        self.max_retry_delay = 60.0
        
        # Track recently placed bounties to avoid duplicates
        self.recent_bounties: Dict[str, float] = {}  # domain -> timestamp
        self.bounty_cooldown = 300  # 5 minutes cooldown per domain
        
    async def start(self) -> None:
        """Start the organizer agent main loop"""
        self.logger.info("Starting Mote Organizer Agent")
        
        # Register and authenticate
        await self._register_agent()
        
        # Main event loop
        await self._run_event_loop()
    
    async def _register_agent(self) -> None:
        """Register the agent with the Mote server"""
        try:
            # Register the agent
            response = await self.client.call_tool("register_agent_simple", {
                "agent_name": "mote-organizer-agent"
            })
            
            self.agent_id = response["agent_id"]
            self.api_token = response["api_token"]
            
            self.logger.info(f"Registered agent: {self.agent_id}")
            
        except Exception as e:
            self.logger.error(f"Failed to register agent: {e}")
            raise
    
    async def _run_event_loop(self) -> None:
        """Main event loop for processing SSE events"""
        while True:
            try:
                await self._subscribe_to_events()
            except Exception as e:
                self.logger.error(f"Event subscription failed: {e}")
                await self._backoff()
    
    async def _subscribe_to_events(self) -> None:
        """Subscribe to SSE events and process them"""
        self.logger.info("Subscribing to SSE events")
        
        # Connect to SSE endpoint
        events_url = f"{self.server_url}/events"
        
        async with self.client.session.get(events_url) as response:
            response.raise_for_status()
            
            self.logger.info("Connected to SSE stream")
            self.retry_delay = 1.0  # Reset retry delay on successful connection
            
            async for line in response.content:
                if line:
                    line_str = line.decode('utf-8').strip()
                    if line_str.startswith('data: '):
                        try:
                            event_data = json.loads(line_str[6:])  # Remove 'data: ' prefix
                            await self._handle_event(event_data)
                        except json.JSONDecodeError as e:
                            self.logger.warning(f"Failed to parse event JSON: {e}")
                        except Exception as e:
                            self.logger.error(f"Error handling event: {e}")
    
    async def _handle_event(self, event_data: Dict[str, Any]) -> None:
        """Handle incoming SSE events"""
        event_type = event_data.get("event_type", "")
        
        if event_type == "bounty_needed":
            await self._handle_bounty_needed(event_data)
        else:
            self.logger.debug(f"Ignoring event type: {event_type}")
    
    async def _handle_bounty_needed(self, event_data: Dict[str, Any]) -> None:
        """Handle bounty_needed events"""
        try:
            data = event_data.get("data", {})
            domain = data.get("domain")
            mean_novelty = data.get("mean_novelty", 0.0)
            
            if not domain:
                self.logger.warning("bounty_needed event missing domain")
                return
            
            self.logger.info(f"Processing bounty_needed for domain '{domain}' (novelty: {mean_novelty:.3f})")
            
            # Check cooldown to avoid spamming bounties
            if self._is_domain_in_cooldown(domain):
                self.logger.info(f"Domain '{domain}' is in cooldown, skipping")
                return
            
            # Find sparse regions and place bounties
            await self._place_bounties_in_domain(domain)
            
        except Exception as e:
            self.logger.error(f"Error handling bounty_needed event: {e}")
    
    def _is_domain_in_cooldown(self, domain: str) -> bool:
        """Check if a domain is in cooldown period"""
        if domain not in self.recent_bounties:
            return False
        
        time_since_bounty = time.time() - self.recent_bounties[domain]
        return time_since_bounty < self.bounty_cooldown
    
    async def _place_bounties_in_domain(self, domain: str) -> None:
        """Find sparse regions in a domain and place bounties"""
        try:
            # Get exploration suggestions for the domain
            suggestions_response = await self.client.call_tool("get_suggestions", {
                "agent_id": self.agent_id,
                "api_token": self.api_token,
                "domain": domain,
                "limit": 5,
                "include_exploration": True
            })
            
            suggestions = suggestions_response.get("suggestions", [])
            
            # Filter for exploration-tagged suggestions
            exploration_suggestions = [
                s for s in suggestions 
                if s.get("source") == "exploration" and s.get("novelty", 0) > 0.5
            ]
            
            self.logger.info(f"Found {len(exploration_suggestions)} exploration candidates")
            
            # Evaluate each candidate and place bounties in sparse regions
            for suggestion in exploration_suggestions[:3]:  # Limit to top 3 candidates
                await self._evaluate_and_place_bounty(domain, suggestion)
                
        except Exception as e:
            self.logger.error(f"Error placing bounties in domain '{domain}': {e}")
    
    async def _evaluate_and_place_bounty(self, domain: str, suggestion: Dict[str, Any]) -> None:
        """Evaluate a suggestion and place a bounty if the region is sparse"""
        try:
            atom_id = suggestion["atom_id"]
            embedding = suggestion.get("embedding", [])
            
            if not embedding:
                self.logger.warning(f"No embedding found for atom {atom_id}")
                return
            
            # Query the cluster around this atom
            cluster_response = await self.client.call_tool("query_cluster", {
                "agent_id": self.agent_id,
                "api_token": self.api_token,
                "vector": embedding,
                "radius": 0.3,
                "limit": 10
            })
            
            cluster_data = cluster_response.get("pheromone_landscape", {})
            atom_count = cluster_data.get("total", 0)
            active_claims = cluster_response.get("active_claims", [])
            
            # Check if region is genuinely sparse (few atoms, no active claims)
            if atom_count < 3 and len(active_claims) == 0:
                await self._place_bounty(domain, suggestion, atom_count)
            else:
                self.logger.debug(f"Region not sparse enough: {atom_count} atoms, {len(active_claims)} claims")
                
        except Exception as e:
            self.logger.error(f"Error evaluating bounty candidate: {e}")
    
    async def _place_bounty(self, domain: str, suggestion: Dict[str, Any], atom_count: int) -> None:
        """Place a bounty atom for the given suggestion"""
        try:
            atom_id = suggestion["atom_id"]
            statement = suggestion["statement"]
            
            # Create bounty statement
            bounty_statement = f"Research needed: Explore the research gap near '{statement}'. This area appears underexplored with only {atom_count} nearby atoms."
            
            # Publish the bounty atom
            bounty_response = await self.client.call_tool("publish_atoms", {
                "agent_id": self.agent_id,
                "api_token": self.api_token,
                "atoms": [{
                    "type": "bounty",
                    "domain": domain,
                    "statement": bounty_statement,
                    "conditions": {},
                    "metrics": None,
                    "parent_ids": [{
                        "atom_id": atom_id,
                        "edge_type": "inspired_by"
                    }]
                }]
            })
            
            published_atoms = bounty_response.get("published_atoms", [])
            if published_atoms:
                bounty_atom_id = published_atoms[0]["atom_id"]
                self.logger.info(f"Placed bounty {bounty_atom_id} in domain '{domain}'")
                
                # Mark domain as having recent bounty
                self.recent_bounties[domain] = time.time()
            else:
                self.logger.warning("Failed to publish bounty atom")
                
        except Exception as e:
            self.logger.error(f"Error placing bounty: {e}")
    
    async def _backoff(self) -> None:
        """Exponential backoff for reconnection"""
        self.logger.info(f"Waiting {self.retry_delay:.1f}s before reconnection attempt")
        await asyncio.sleep(self.retry_delay)
        
        # Exponential backoff with cap
        self.retry_delay = min(self.retry_delay * 2, self.max_retry_delay)


def main():
    """Main entry point"""
    # Get configuration from environment variables
    server_url = os.getenv("MOTE_SERVER_URL", "http://localhost:3000")
    private_key_pem = os.getenv("MOTE_AGENT_PRIVATE_KEY")
    public_key_pem = os.getenv("MOTE_AGENT_PUBLIC_KEY")
    
    if not private_key_pem or not public_key_pem:
        print("Error: MOTE_AGENT_PRIVATE_KEY and MOTE_AGENT_PUBLIC_KEY environment variables must be set")
        sys.exit(1)
    
    # Create and start the organizer agent
    agent = OrganizerAgent(server_url, private_key_pem, public_key_pem)
    
    try:
        asyncio.run(agent.start())
    except KeyboardInterrupt:
        print("\nShutting down organizer agent...")
    except Exception as e:
        print(f"Fatal error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
