use std::borrow::BorrowMut;
use std::cell::Cell;
use std::collections::HashMap;
use std::error::Error;
use std::ops::Deref;
use std::sync::{Arc, RwLock};

use no_proto::NP_Factory;
use serde::{Deserialize, Serialize};

use crate::artifact::{Artifact, ArtifactBundle, ArtifactCache, ArtifactCacher, ArtifactRepository, ArtifactYaml};

pub struct Configs
{
    pub artifact_cache: Arc<dyn ArtifactCache+Sync+Send>,
    pub buffer_factory_keeper: Keeper<NP_Factory<'static>>,
    pub sim_config_keeper: Keeper<SimConfig>,
    pub tron_config_keeper: Keeper<TronConfig>,
    pub mechtron_config_keeper: Keeper<MechtronConfig>,
}

impl Configs{

    pub fn new(artifact_source:Arc<dyn ArtifactCache+Sync+Send>)->Self
    {
        Configs{
            artifact_cache: artifact_source.clone(),
            buffer_factory_keeper: Keeper::new(artifact_source.clone() , Box::new(NP_Buffer_Factory_Parser )),
            sim_config_keeper: Keeper::new(artifact_source.clone(), Box::new( SimConfigParser )),
            tron_config_keeper: Keeper::new(artifact_source.clone(), Box::new( TronConfigParser )),
            mechtron_config_keeper: Keeper::new(artifact_source.clone(), Box::new( MechtronConfigParser ))
        }
    }
}



pub struct Keeper<V>
{
    config_cache: RwLock<HashMap<Artifact,Arc<V>>>,
    repo: Arc<dyn ArtifactCache+Send+Sync>,
    parser: Box<dyn Parser<V> + Send+Sync>
}

impl <V> Keeper<V>
{
    pub fn new(repo: Arc<dyn ArtifactCache + Send + Sync>, parser: Box<dyn Parser<V> + Send + Sync>) -> Self
    {
        Keeper {
            config_cache: RwLock::new(HashMap::new()),
            parser: parser,
            repo: repo
        }
    }

    pub fn cache(&mut self, artifact: &Artifact ) ->Result<(),Box<dyn Error + '_>>
    {
        let mut cache = self.config_cache.write()?;

        if cache.contains_key(artifact)
        {
            return Ok(());
        }

        self.repo.cache(&artifact);


        let str = self.repo.get(&artifact)?;

        let value = self.parser.parse(&artifact, str.as_ref())?;
        cache.insert( artifact.clone(), Arc::new(value) );
        Ok(())
    }

    pub fn get( &self, artifact: &Artifact ) -> Result<Arc<V>,Box<dyn Error + '_>>
    {
        let cache = self.config_cache.read()?;
        match cache.get(&artifact)
        {
            None => Err(format!("could not find config for artifact: {}",artifact.to()).into()),
            Some(value) => Ok(value.clone())
        }
    }
}

pub trait Parser<V>
{
    fn parse( &self, artifact: &Artifact, str: &str )->Result<V,Box<dyn Error>>;
}


struct NP_Buffer_Factory_Parser;

impl <'fact> Parser<NP_Factory<'fact>> for NP_Buffer_Factory_Parser
{
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<NP_Factory<'fact>, Box<dyn Error>> {
        let result = NP_Factory::new(str);
        match result {
            Ok(rtn) => Ok(rtn),
            Err(e) => Err(format!("could not parse np_factory from artifact: {}",artifact.to()).into())
        }
    }
}

struct SimConfigParser;

impl Parser<SimConfig> for SimConfigParser
{
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<SimConfig, Box<dyn Error>> {
        let sim_config_yaml = SimConfigYaml::from(str)?;
        let sim_config = sim_config_yaml.to_config(artifact)?;
        Ok(sim_config)
    }
}

struct MechtronConfigParser;

impl Parser<MechtronConfig> for MechtronConfigParser
{
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<MechtronConfig, Box<dyn Error>> {
        let mechtron_config_yaml = MechtronConfigYaml::from_yaml(str)?;
        let mechtron_config = mechtron_config_yaml.to_config(artifact)?;
        Ok(mechtron_config)
    }
}

struct TronConfigParser;

impl Parser<TronConfig> for TronConfigParser
{
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<TronConfig, Box<dyn Error>> {
        let tron_config_yaml = TronConfigYaml::from_yaml(str)?;
        let tron_config = tron_config_yaml.to_config(artifact)?;
        Ok(tron_config)
    }
}


pub struct MechtronConfig {
    pub source: Artifact,
    pub wasm: Artifact,
    pub tron_config: Artifact
}

pub struct TronConfig{
    pub kind: Option<String>,
    pub name: String,
    pub nucleus_lookup_name: Option<String>,
    pub source: Artifact,
    pub content: Option<ContentConfig>,
    pub messages: Option<MessagesConfig>
}

pub struct MessagesConfig
{
    pub create: Option<CreateMessageConfig>
}

pub struct ContentConfig
{
    pub artifact: Artifact
}

pub struct CreateMessageConfig
{
    pub artifact: Artifact
}

pub struct InMessageConfig
{
    pub name: String,
    pub phase: Option<String>,
    pub artifact: Vec<Artifact>
}

pub struct OutMessageConfig
{
   pub name: String,
   pub artifact: Artifact
}

impl ArtifactCacher for TronConfig{
    fn cache(&self, configs: &mut Configs ) -> Result<(), Box<dyn Error>> {

        if self.content.is_some() {
            configs.buffer_factory_keeper.cache( &self.content.as_ref().unwrap().artifact );
        }

        if self.messages.is_some() && self.messages.as_ref().unwrap().create.is_some(){
            configs.buffer_factory_keeper.cache( &self.messages.as_ref().unwrap().create.as_ref().unwrap().artifact );
        }

       Ok(())
    }
}

impl ArtifactCacher for MechtronConfig {
    fn cache(&self, configs: &mut Configs) -> Result<(), Box<dyn Error>> {
       configs.tron_config_keeper.cache(&self.tron_config);
       Ok(())
    }
}


#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MechtronConfigYaml
{
    name: String,
    wasm: ArtifactYaml,
    tron_config: ArtifactYaml
}

impl MechtronConfigYaml {

    pub fn from_yaml(string:&str) -> Result<Self,Box<dyn Error>>
    {
        Ok(serde_yaml::from_str(string )?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<MechtronConfig,Box<dyn Error>>
    {
        let default_bundle = &artifact.bundle.clone();
        return Ok( MechtronConfig {
            source: artifact.clone(),
            wasm: self.wasm.to_artifact(default_bundle)?,
            tron_config: self.tron_config.to_artifact(default_bundle)?,
        } )
    }
}



#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TronConfigYaml
{
    kind: Option<String>,
    name: String,
    nucleus_lookup_name: Option<String>,
    content: Option<ContentConfigYaml>,
    messages: Option<MessagesConfigYaml>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesConfigYaml
{
    create: Option<CreateConfigYaml>,
    inbound: Option<InboundConfigYaml>,
    outbound: Option<Vec<OutMessageConfigYaml>>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InboundConfigYaml
{
    ports: Vec<PortConfigYaml>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboundConfigYaml
{
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateConfigYaml
{
  artifact: ArtifactYaml
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentConfigYaml
{
    artifact: ArtifactYaml
}


#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PortConfigYaml
{
    name: String,
    phase: Option<String>,
    artifact: ArtifactYaml
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OutMessageConfigYaml
{
    name: String,
    artifact: ArtifactYaml
}

impl TronConfigYaml {

    pub fn from_yaml(string:&str) -> Result<Self,Box<dyn Error>>
    {
        Ok(serde_yaml::from_str(string )?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<TronConfig,Box<dyn Error>>
    {
        let default_bundle = &artifact.bundle.clone();




        return Ok( TronConfig{
            kind: self.kind.clone(),
            source: artifact.clone(),
            name: self.name.clone(),

            messages: match &self.messages{
            None=>Option::None,
            Some(messages)=>Option::Some( MessagesConfig{
            create: match &messages.create {
                None=>Option::None,
                Some(create)=>Option::Some(CreateMessageConfig{artifact:create.artifact.to_artifact(default_bundle)?})
            }})},
            content: match &self.content{
                Some(content)=>Option::Some( ContentConfig{ artifact: content.artifact.to_artifact(default_bundle)?} ),
                None=>Option::None,
            },
            nucleus_lookup_name: None
        } )
    }
}


pub struct SimConfig{
    source: Artifact,
    name: String,
    main: Artifact,
    create_message: Artifact
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SimConfigYaml
{
    name: String,
    main: ArtifactYaml,
    create_message: ArtifactYaml
}

impl SimConfigYaml
{
    pub fn from(string:&str) -> Result<Self,Box<dyn Error>>
    {
        Ok(serde_yaml::from_str(string )?)
    }

    pub fn to_config(&self, artifact: &Artifact ) -> Result<SimConfig,Box<dyn Error>>
    {
        let default_artifact = &artifact.bundle.clone();
        Ok( SimConfig{
            source: artifact.clone(),
            name: self.name.clone(),
            main: self.main.to_artifact(default_artifact)?,
            create_message: self.create_message.to_artifact(default_artifact)?
        } )
    }
}

impl ArtifactCacher for SimConfig {
    fn cache(&self, configs: &mut Configs) -> Result<(), Box<dyn Error>> {
/*        let configs = (*configs).get_mut();
        configs.buffer_factory_keeper.cache(&self.create_message);
        configs.mechtron_config_keeper.cache(&self.main);

 */
        Ok(())
    }
}

