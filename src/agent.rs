/*
TODO: The interest vector is initially random. At every time step, the agents increase or decrease the
interest in a market based on other agents' (close in the network) beliefs, overall profits from that
asset, and other news (random noise in our case).
TODO: Fundamentalists? From the DGA paper.
*/

use std::{collections::VecDeque, iter::repeat_with, ops::Div};

use rand::{distributions::Uniform, prelude::Rng, prelude::ThreadRng, thread_rng};
use rand_distr::Standard;
use smallvec::SmallVec;

use crate::{
    config::Config,
    market::{GenoaMarket, MarketId},
};

pub type AgentId = usize;

#[derive(Debug, Clone)]
pub struct Agent<const M: usize> {
    pub cash: f32,
    /// Vector representing the amount of assets an agent holds.
    pub assets: SmallVec<[u32; M]>,

    // /// Value that represents the market in which an agent invests next.
    // market_preference: u32,
    /// Vector encapsulating each market preference of an agent. Contains probabilities between [0, 1].
    pub state: SmallVec<[f32; M]>,

    // /// Variable describing how likely an agent is to make informed decisions vs following the crowd.
    // fundamentalism_ratio: f32,
    /// Value that describes how likely and agent is to place orders at each time step.
    order_probability: SmallVec<[f32; M]>,

    /// Value that describes how likely and agent is to be influenced at each time step.
    influence_probability: f32,

    /// Value that determines how many agents a single agent should be influenced from.
    influencers_count: usize,

    #[allow(unused)]
    reflection_delay: usize,
    influences: VecDeque<Influence>,
    friend_threshold: f32,
    max_friends: usize,
    friend_influence_probability: f32,

    /// Friend list containing trust values for other agents.
    pub friends: VecDeque<Friend>,
    // /// Value that describes how likely an agent is to change its preferences.
    // change_probability: f32,
}

impl<const M: usize> Agent<M> {
    pub fn new(config: &Config, rng: &mut ThreadRng) -> Agent<M> {
        Agent {
            cash: config.agent.initial_cash.sample_f32(rng),
            // market_preference: 0,
            assets: repeat_with(|| config.agent.initial_assets.sample_usize(rng) as u32)
                .take(config.market.market_count)
                .collect(),
            state: repeat_with(|| config.agent.initial_state.sample_f32(rng))
                .take(config.market.market_count)
                .collect(),
            // fundamentalism_ratio: 0.35,
            order_probability: repeat_with(|| config.agent.order_probability.sample_f32(rng))
                .take(config.market.market_count)
                .collect(),
            influence_probability: config.agent.influence_probability.sample_f32(rng),
            influencers_count: config.agent.influencers_count.sample_usize(rng),
            reflection_delay: config.agent.reflection_delay.sample_usize(rng),
            influences: VecDeque::new(),
            friend_threshold: config.agent.friend_threshold.sample_f32(rng),
            friends: VecDeque::new(),
            max_friends: config.agent.max_friends.sample_usize(rng),
            friend_influence_probability: config.agent.friend_influence_probability.sample_f32(rng),
        }
    }

    pub fn apply_buy(&mut self, market: MarketId, asset_quantity: u32, price_per_item: f32) {
        self.cash -= price_per_item * asset_quantity as f32;

        assert!(self.cash >= 0.0, "Agent ran out of cash");

        self.assets[market] += asset_quantity;
    }

    pub fn apply_sell(&mut self, market: MarketId, asset_quantity: u32, price_per_item: f32) {
        self.cash += price_per_item * asset_quantity as f32;

        let a = &mut self.assets[market];
        *a = a
            .checked_sub(asset_quantity)
            .expect("Agent ran out of asset");
    }
}

#[derive(Debug, Clone)]
pub struct Influence {
    influencer: AgentId,
    state: Vec<f32>,
    #[allow(unused)]
    step: usize,
}

#[derive(Debug, Clone)]
pub struct Friend {
    agent: AgentId,
    score: i32,
}

#[derive(Debug, Clone)]
pub struct AgentCollection<const M: usize> {
    agents: Vec<Agent<M>>,
    fundamentalists: Vec<SmallVec<[f32; M]>>,
}

impl<const M: usize> AgentCollection<M> {
    pub fn new(config: &Config) -> AgentCollection<M> {
        let mut rng = thread_rng();
        AgentCollection {
            agents: repeat_with(|| Agent::new(config, &mut rng))
                .take(config.agent.agent_count)
                .collect(),
            fundamentalists: repeat_with(|| {
                thread_rng()
                    .sample_iter(Standard)
                    .map(|b: bool| b as usize as f32)
                    .take(config.market.market_count)
                    .collect()
            })
            .take(config.agent.fundamentalist_count)
            .collect(),
        }
    }

    pub fn agent(&self, id: AgentId) -> &Agent<M> {
        &self.agents[id as usize]
    }

    pub fn agent_mut(&mut self, id: AgentId) -> &mut Agent<M> {
        &mut self.agents[id as usize]
    }

    pub fn agents(&self) -> &[Agent<M>] {
        &self.agents[..]
    }

    /// Call this function first, once every step.
    pub fn step(&mut self, markets: &[GenoaMarket], step: usize) {
        self.dga(markets, step);
    }

    /// Call this function after [`Self::step`], once for every market.
    pub fn step_market(&mut self, market: &mut GenoaMarket) {
        self.trade_on_market(market);
    }

    pub fn total_cash(&self) -> f64 {
        self.agents.iter().map(|a| a.cash as f64).sum()
    }

    pub fn total_assets(&self, market: MarketId) -> u32 {
        self.agents.iter().map(|a| a.assets[market]).sum()
    }

    pub fn mean_state(&self, market: MarketId) -> f32 {
        let states = self
            .agents
            .iter()
            .map(|a| a.state[market] as f32)
            .sum::<f32>();
        states / self.agents.len() as f32
    }

    pub fn cash_median(&self) -> f32 {
        let mut cash: Vec<_> = self.agents.iter().map(|a| a.cash).collect();
        cash.sort_by(|a, b| a.partial_cmp(b).unwrap());
        cash[cash.len() / 2]
    }

    pub fn wealth_median(&self, markets: &[GenoaMarket]) -> f32 {
        let mut wealth: Vec<_> = self
            .agents
            .iter()
            .map(|a| {
                a.cash
                    + a.assets
                        .iter()
                        .zip(markets)
                        .map(|(&a, m)| a as f32 * m.price())
                        .sum::<f32>()
            })
            .collect();
        wealth.sort_by(|a, b| a.partial_cmp(b).unwrap());
        wealth[wealth.len() / 2]
    }

    /// Give the state of either an agent or a fundamentalist. `idx` must be in
    /// range 0 to len(agents) + len(fundamentalists)
    pub fn influence_at_market(&self, idx: usize, market: MarketId) -> f32 {
        self.influence_at(idx)[market]
    }

    pub fn influence_at(&self, idx: usize) -> &[f32] {
        if idx < self.agents.len() {
            &self.agents[idx].state
        } else {
            &self.fundamentalists[idx - self.agents.len()]
        }
    }

    /// Every agent updates their beliefs based on other agents' preferences
    /// and their own interests. At every time step, the interest for a market
    /// is updated based on performance (overall profits from a market), news
    /// and random noise.
    pub fn dga(&mut self, markets: &[GenoaMarket], step: usize) {
        // More efficient this way.
        let mut rng = thread_rng();

        let range = Uniform::from(0..self.agents.len() + self.fundamentalists.len());
        let market_count = markets.len();

        for idx in 0..self.agents.len() {
            // Check if the current agent is to be influenced based on the influence probability.
            if rng.gen::<f32>() < self.agents[idx].influence_probability {
                // Generate our influencers
                let mut influencers = rng
                    .clone()
                    .sample_iter(&range)
                    // Make sure we do not influence ourselves
                    .filter(|&i: &usize| i != idx)
                    .take(self.agents[idx].influencers_count)
                    .collect::<Vec<_>>();

                // Also be influenced by friends
                for f in &self.agents[idx].friends {
                    if rng.gen::<f32>() < self.agents[idx].friend_influence_probability {
                        influencers.push(f.agent);
                    }
                }

                // Influence all markets
                for market in 0..market_count {
                    // Take random agents and sum their influence.
                    let influence_sum = influencers
                        .iter()
                        .map(|&i| self.influence_at_market(i, market))
                        .sum::<f32>();

                    let influence = influence_sum / influencers.len() as f32;

                    self.agents[idx].state[market] = influence.round();
                }

                for i in influencers {
                    let influence = self.influence_at(i).to_vec();
                    self.agents[idx].influences.push_back(Influence {
                        influencer: i,
                        state: influence,
                        step,
                    });
                }
            }
        }
    }

    /// Checks the performance of friends and influencers based on the previous time step.
    pub fn update_friends(&mut self, markets: &[GenoaMarket]) {
        let market_count = markets.len();

        // Calculate the market movements of all the markets from the previous step.
        let market_movement = markets
            .iter()
            .map(|m| m.price() - m.price_ago(1))
            .collect::<Vec<_>>();
            
        // Caculate the mean of the market movements.
        let market_movement_mean = market_movement.iter().sum::<f32>().div(market_count as f32);

        // Calculate the standard deviations of the market movements.
        let market_movement_sd = market_movement
            .iter()
            .map(|x| (x - market_movement_mean) * (x - market_movement_mean))
            .sum::<f32>()
            .div((market_count - 1) as f32)
            .sqrt();

        for idx in 0..self.agents.len() {
            let agent = &mut self.agents[idx];
            let influences = &mut agent.influences;

            // Iterate over influences of the previous step for this agent.
            while influences.front().is_some() {
                let i = influences.pop_front().unwrap();

                let influence_mean = i.state.iter().sum::<f32>().div(market_count as f32);

                let influence_sd = i
                    .state
                    .iter()
                    .map(|x| (x - influence_mean) * (x - influence_mean))
                    .sum::<f32>()
                    .div((market_count - 1) as f32)
                    .sqrt();

                // Calculate the correlation between the market movements and states of influencer.
                let correlation = market_movement
                    .iter()
                    .zip(i.state.iter())
                    .map(|(&mm, &i)| (mm - market_movement_mean) * (i - influence_mean))
                    .sum::<f32>()
                    .div(influence_sd * market_movement_sd)
                    .div((market_count - 1) as f32);

                let mut is_friend = false;

                // Iterate over the agent's friends
                for (idf, friend) in agent.friends.iter_mut().enumerate() {
                    // Check if the current influencer is a friend.
                    if friend.agent == i.influencer {
                        is_friend = true;

                        // Update friend score.
                        if correlation > agent.friend_threshold {
                            friend.score += 1
                        } else {
                            friend.score -= 1
                        }

                        // Remove friend if score falls below 0.
                        if friend.score < 0 {
                            agent.friends.remove(idf);
                        }
                        break;
                    }
                }

                // If current influencer is not a friend yet, his performance is good and there is
                // space in the friend list of the agent; Add the influencer as friend.
                if !is_friend
                    && correlation > agent.friend_threshold
                    && agent.friends.len() < agent.max_friends
                {
                    agent.friends.push_back(Friend {
                        agent: i.influencer,
                        score: 1,
                    })
                }
            }
        }
    }

    pub fn trade_on_market(&mut self, market: &mut GenoaMarket) {
        let mut rng = thread_rng();

        for (agent_id, agent) in self.agents.iter_mut().enumerate() {
            let agent_id = agent_id as AgentId;
            let m_id = market.id();

            if rng.gen::<f32>() < agent.order_probability[m_id] {
                if rng.gen::<f32>() < agent.state[m_id] {
                    market.buy_order(agent_id, agent.cash * rng.gen::<f32>());
                } else {
                    let assets = agent.assets[m_id] as f32 * rng.gen::<f32>();
                    market.sell_order(agent_id, assets as u32)
                }
            }
        }
    }
}
