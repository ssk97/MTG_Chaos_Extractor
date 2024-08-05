use serde_json::Value;
use std::{fs, fs::File, path::Path, io::{BufReader, BufWriter, Write, BufRead}, collections::{HashSet, HashMap, BTreeMap}, sync::OnceLock};
use regex::Regex;
use deunicode::deunicode;
//const MASTER_LIST:[&str;13] = ["MMA","VMA","MM2","EMA","MM3","IMA","UMA","2XM","2X2","MH1","MH2","A25","DMR"];
//const SET_LIST:[&str;3] = ["RTR","GTC","DGM"];
const CORE_SKIP:[&str;4] = ["7ED","8ED","9ED","10E"];
const FILE_OUT_SCRYFALL:&str = "Tentpole_scryfall.txt";
const FILE_OUT_FINAL:&str = "Tentpole.txt";

const FILE_OUT_MAGE:&str = "EVERYTHING_mage.txt";
const PRELUDE:&[u8] = include_bytes!("DEFAULT_prelude.txt");
#[derive(PartialEq, PartialOrd, Copy, Clone, Debug)]
enum DupMode{
    All, NoId, PerSet, Canonicalize, Latest
}
const DUPLICATE_MODE:DupMode = DupMode::PerSet;

fn main(){
    scryfall_data();
    //mage_compatible();
    set_intersect();
}

#[derive(PartialEq, PartialOrd, Eq, Ord, Copy, Clone, Debug)]
enum Rarity{
    Land,
    Common,
    Uncommon,
    Rare,
    Mythic,
    Other,
}
use Rarity::*;
//const RARITY_ARR:[Rarity;5] = [Land,Common,Uncommon,Rare,Mythic];
//const RARITY_ARR:[Rarity;2] = [Common,Uncommon];
const RARITY_ARR:[Rarity;5] = [Common,Uncommon,Rare,Mythic,Other];

#[derive(PartialEq, Clone, Debug)]
struct CardData{
    rarity: Rarity,
    count: i32,
    date: String,
    name: String,
}

struct SetData {
    datamap:BTreeMap<String, CardData>,//Unique card name, (fullname, rarity, multiplier)
}

fn is_land(card: &Value) -> bool{
    if card["type_line"].to_string().contains("Land"){
        let layout = card["layout"].as_str().unwrap();
        if ["normal","modal_dfc"].contains(&layout) {return true;}
        if let Some(Value::Array(faces)) = card.get("card_faces") {
            let face = faces[0].as_object().unwrap();
            if face["type_line"].as_str().unwrap().contains("Land") {return true;}
        }
    }
    return false;
}

fn rarity_check(card: &Value, rarity: &Rarity) -> bool{
    /*if is_land(card){
        return rarity == &Land
    }*/
    let rarity_val = &match rarity{
        Land => return false,
        Common =>   "common",
        Uncommon => "uncommon",
        Rare =>     "rare",
        Mythic =>   "mythic",
        Other => return true,
    };
    let card_rarity = &card["rarity"];
    return card_rarity == rarity_val;
}
fn get_mult(_card: &Value) -> i32 {return 1;}
/*fn get_mult(card: &Value) -> i32 {
    if is_land(card) {return 1;}
    if let Some(Value::Array(colorid)) = card.get("color_identity"){
        if colorid.len() == 0 {return 1}
    }
    if let Some(Value::Array(colors)) = card.get("colors"){
        if colors.len() == 0 {return 2}
    }
    return 4;
}
fn get_mult(card: &Value) -> i32{
    let card_rarity = &card["rarity"];
    if !is_land(card){
        return 1;
    }
    if card["color_identity"].as_array().unwrap().len() == 2
        && card["produced_mana"].as_array().unwrap().len() >= 2 {return 8};
    return match card_rarity.as_str().unwrap(){
        "common"    =>8,
        "uncommon"  =>4,
        "rare"      =>2,
        "mythic"    =>1,
        _ => 0
    };
}*/
fn check_sets(card: &Value) -> bool{

    let set = card["set"].as_str().unwrap().to_ascii_uppercase();
    if set == "MH1" {return true;}

    let date = card["released_at"].as_str().unwrap();
    let set_type = card["set_type"].as_str().unwrap();
    if !["core","expansion"].contains(&set_type) {return false;}
    if date[0..4] < *"2000" || (date[0..4] == *"2000" && date[5..7] < *"09") {return false;}
    if date[0..4] > *"2020" || (date[0..4] == *"2020" && date[5..7] > *"02") {return false;}
    if card["booster"]==false {return false;}
    return !CORE_SKIP.contains(&set.as_str());
    //let set = card["set"].as_str().unwrap().to_ascii_uppercase();
    //return SET_LIST.contains(&set.as_str());
}

//The set of checks that are basically guaranteed to be done for all formats, making sure the cards are real/legal MTG cards
fn baseline_check(card: &Value) -> bool{
    assert!(card["object"] == "card");
    if card.get("mtgo_id").is_none() && card.get("arena_id").is_none()
    && card.get("tcgplayer_id").is_none()/* && card.get("cardmarket_id").is_none()*/{
        //The card isn't a normal card - Meld backsides, for example
        return false;
    }
    if let Value::Object(legal) = &card["legalities"]{
        let set_type = card["set_type"].as_str().unwrap();
        if ["memorabilia","token","promo"].contains(&set_type) {return false;}
        let layout = card["layout"].as_str().unwrap();
        const BANNED_LAYOUTS:[&str;8] = ["reversible_card","art_series","emblem","double_faced_token","token","vanguard","scheme","planar"];
        if BANNED_LAYOUTS.contains(&layout) {return false;}
        if card["lang"] != "en" {return false;}
        if !(card["games"].as_array().unwrap().iter().any(|x| x == "paper")) {return false;}
        if legal["vintage"] == "not_legal" || legal["vintage"] == "banned" {return false;}
        if card["type_line"].as_str().unwrap().contains("Basic") {return false;}
        return true;
    }
    return false;
}

//Remove Monarch, Initiative, and Commander cards
static REGEX_REMOVE_PARENS: OnceLock<Regex> = OnceLock::new();
fn mechanics_check(card: &Value) -> bool{
    let full_oracle = card.get("oracle_text").map_or_else(||{None},Value::as_str).unwrap_or("");
    let regex = REGEX_REMOVE_PARENS.get_or_init(||{
        Regex::new(r"\([^)]+\)").unwrap()
    });
    let text_str = regex.replace_all(full_oracle, "").to_lowercase();
    if text_str.contains("the monarch") || text_str.contains("the initiative") {return false;}
    if text_str.contains("commander"){
        if card["name"].as_str().unwrap().contains("Commander"){
            return card["type_line"].as_str().unwrap().contains("Creature");
        } else {
            return card["type_line"].as_str().unwrap().contains("Planeswalker");
        }
    }
    return true;
}

fn check_supplemental(card: &Value) -> bool{
    let set_id:String = card["set"].as_str().unwrap().to_ascii_uppercase();
    let set_type = card["set_type"].as_str().unwrap();
    if ["PLST","PLIST","PHED","PAGL"].contains(&set_id.as_str()) {return false;}
    if ["treasure_chest"].contains(&set_type) {return false;}
    return true;
}
fn check_modern(card: &Value) -> bool{
    let date = card["released_at"].as_str().unwrap();
    if let Value::Object(legal) = &card["legalities"]{
        if legal["modern"] != "legal" { return false; }
    } else {
        return false;
    }
    if card["booster"] != true {return false;}
    let set_id:String = card["set"].as_str().unwrap().to_ascii_uppercase();
    if ["PLST","PLIST","PHED","PAGL","DBL"].contains(&set_id.as_str()) {return false;}
    //if ["LTR","MH1","MH2","J22"].contains(&set_id.as_str()) {return true;}
    //if ["CMM","MB1","PLST","PLIST","FMB1","SLX"].contains(&set_id.as_str()) {return false;}
    //if !["core","expansion","masters"].contains(&set_type) {return false;}
    let year:u16 = (date[0..4]).parse().unwrap();
    return year >= 2003;
    //if !["core","expansion","draft_innovation","masters"].contains(&set_type) {return false;}
}
//Ban all versions of a given card
fn ban_check(card: &Value) -> bool {return false;}
/*fn ban_check(card: &Value) -> bool{
    let set_id:String = card["set"].as_str().unwrap().to_ascii_uppercase();
    let set_type = card["set_type"].as_str().unwrap();
    if card["booster"]==false {return false;}
    if ["MAT"].contains(&set_id.as_str()) {return false;}
    if ["core","expansion"].contains(&set_type) {return true;}
    if ["POR","P02","PTK","LTR","MH1","MH2"].contains(&set_id.as_str()) {return true;}
    return false;
}*/
/*fn check_gold(card: &Value) -> bool{
    if let Some(Value::Array(colorid)) = card.get("color_identity"){
        if colorid.len() == 1 {return false;}
        if is_land(card){
            return true;
        }
        if let Some(Value::Array(colors)) = card.get("colors"){
            return colors.len() != 1;
        }
    }
    return false;
}*/


fn get_simplename(card: &serde_json::Value) -> &str{
    if let Some(Value::Array(faces)) = card.get("card_faces") {
        if card["layout"] != "split"{
            let face = faces[0].as_object().unwrap();
            return face["name"].as_str().unwrap();
        }
    }
    return card["name"].as_str().unwrap();

}
fn _print_card(card: &Value){
    println!("\n\nCARD");
    for (k, v) in card.as_object().unwrap().iter(){
        println!("{}, {:?}", k, v);
    }
}

fn scryfall_data() {
    let file_in = r"src/default-cards.json";
    let file_in_singlecard = r"src/oracle-cards.json";
    println!("loading!");
    let mut canon_name = HashMap::new();

    //if Canonicalizing names, load the oracle cards database and set up a card name->fullname mapping
    if DUPLICATE_MODE == DupMode::Canonicalize{
        let file_read2 = BufReader::new(File::open(Path::new(file_in_singlecard)).unwrap());
        let single_card_json = serde_json::from_reader(file_read2).unwrap();
        if let Value::Array(cards) = single_card_json{
            println!("json 1 loaded {} cards!", cards.len());
            for card in cards.iter(){
                if baseline_check(card){
                    let name = get_simplename(card).to_string();
                    let set = card["set"].as_str().unwrap().to_ascii_uppercase();
                    let id = card["collector_number"].as_str().unwrap();
                    let fullname = format!("{} ({}) {}",name, set, id);
                    assert!(card["object"] == "card");
                    canon_name.insert(name, fullname);
                }
            }
        }else {
            panic!("File not array of cards");
        }
    }
    
    let file_read = BufReader::new(File::open(Path::new(file_in)).unwrap());
    let v = serde_json::from_reader(file_read).unwrap();
    let mut file_write =  BufWriter::new(File::create(Path::new(FILE_OUT_SCRYFALL)).unwrap());

    let mut count = 0;
    let mut seen_map = SetData::new();
    let mut ban_list = HashSet::new();
    //read in
    if let Value::Array(v) = v{
        println!("json 2 loaded {} cards!", v.len());
        for rarity in RARITY_ARR{
            for card in v.iter(){
                let (mapname, cardmapvalue) = make_card_data(card, rarity, &canon_name);
                if ban_check(card) {
                    ban_list.insert(mapname.clone());
                }
                if baseline_check(card) && rarity_check(card, &rarity) && check_sets(card) {
                    if mechanics_check(card) {
                        seen_map.insert(mapname, cardmapvalue);
                        count += 1;
                    } else {
                        println!("mechanics fail: {}",cardmapvalue.name);
                    }
                }
            }
        }
    } else {
        panic!("File not array of cards");
    }
    println!("Scryfall Complete with {} cards, saving", count);
    seen_map.filter(&ban_list);
    if seen_map.datamap.len() != count{
        println!("{} cards banned", count-seen_map.datamap.len());
    }
    for rarity in RARITY_ARR{
        file_write.write_all(format!("[{:?}]\n",rarity).as_bytes()).unwrap();
        seen_map.foreach(rarity, |fullname, multiplier|
            {
                if multiplier == 1{
                    file_write.write_all(format!("{}\n", fullname).as_bytes()).unwrap();
                } else if multiplier > 0{
                    file_write.write_all(format!("{} {}\n",multiplier, fullname).as_bytes()).unwrap();
                }
            }
        );
    }
    println!("File output complete!");
}

fn mage_compatible(){
    let directory = r"src/sets";
    let files = fs::read_dir(Path::new(directory)).unwrap();

    let file_out = FILE_OUT_MAGE;
    let mut file_write =  BufWriter::new(File::create(Path::new(file_out)).unwrap());

    let card_name_regex = Regex::new(r#""(.*?)"\s*(,|\)|$)"#).unwrap();
    let card_regex =  Regex::new(r"SetCardInfo").unwrap();
    let find_set_regex = Regex::new(r#".*?super\(".*?",\s*"(.*?)","#).unwrap();

    let mut count = 0;
    let mut seen_map = HashSet::new();

    for file in files{
        let file = file.unwrap();
        assert!(file.file_type().unwrap().is_file());
        let read = BufReader::new(File::open(file.path()).unwrap());
        let mut set = None;
        'reading: for line_check in read.lines(){
            let data = line_check.unwrap();
            if card_regex.is_match(&data){
                if !set.is_some() {break 'reading;}
                let name_capture = card_name_regex.captures(&data);
                let name = &name_capture.unwrap()[1].replace("\\","");
                //let full_name = format!("{} ({})\n",name,set.as_ref().unwrap());
                let full_name = format!("{}\n",name);
                if !_UNIMPLEMENTED.contains(&name.as_str()){
                    if seen_map.insert(full_name.to_string()){
                        file_write.write(full_name.as_bytes()).unwrap();
                        //file_write.write(name.as_bytes()).unwrap();
                        //file_write.write("\n".as_bytes()).unwrap();
                        count += 1;
                    }
                } else {
                    println!("excluded: {}",name);
                }
            } else {
                let caps = find_set_regex.captures(&data);
                if let Some(caps) = caps{
                    let set_str = caps[1].to_ascii_uppercase().to_string();
                    set = Some(set_str);
                }
            }
        }
    }
    println!("Mage complete with {} cards, saving", count);
}
const _UNIMPLEMENTED: [&str;40] = [
"Archipelagore",
"Auspicious Starrix",
"Boneyard Lurker",
"Brokkos, Apex of Forever",
"Cavern Whisperer",
"Chittering Harvester",
"Cloudpiercer",
"Cubwarden",
"Dirge Bat",
"Dreamtail Heron",
"Everquill Phoenix",
"Gemrazer",
"Glowstone Recluse",
"Huntmaster Liger",
"Illuna, Apex of Wishes",
"Insatiable Hemophage",
"Lore Drakkis",
"Majestic Auricorn",
"Migratory Greathorn",
"Necropanther",
"Nethroi, Apex of Death",
"Parcelbeast",
"Porcuparrot",
"Pouncing Shoreshark",
"Regal Leosaur",
"Sea-Dasher Octopus",
"Snapdax, Apex of the Hunt",
"Trumpeting Gnarr",
"Vadrok, Apex of Thunder",
"Vulpikeet",

"Mindleecher",
"Otrimi, the Ever-Playful",
"Souvenir Snatcher",
"Sawtusk Demolisher",

"Path of the Animist",
"Path of the Enigma",
"Path of the Ghosthunter",
"Path of the Pyromancer",
"Path of the Schemer",

"Spy Kit",
];

fn set_intersect(){
    //let single_card = Regex::new(r"^(\[.{3,5}\])(.*)//(.*)$").unwrap();
    //let mut half_cards = HashSet::new();
    //let mut half_mapping = HashMap::new();

    let read1 = BufReader::new(File::open(Path::new(FILE_OUT_MAGE)).unwrap());
    let mut xmage = HashSet::new();
    for line in read1.lines(){
        let line = line.unwrap();
        xmage.insert(deunicode(line.trim()));
    }
    let read2 = BufReader::new(File::open(Path::new(FILE_OUT_SCRYFALL)).unwrap());
    let mut scryfall = HashSet::new();

    let mut file_write =  BufWriter::new(File::create(Path::new(FILE_OUT_FINAL)).unwrap());

    /*let intersect = xmage.intersection(&scryfall);
    for item in intersect{
        //file_write.write(item.as_bytes()).unwrap();
        file_write.write(check_expand(item, &card_real_name).as_bytes()).unwrap();
        file_write.write("\n".as_bytes()).unwrap();
    }*/
    
    let scryfall_short_card_regex = match DUPLICATE_MODE {
        DupMode::Canonicalize | DupMode::All | DupMode::PerSet => {
            Regex::new(r"^(\d+ )?([^0-9].*) \(.*\) .+$").unwrap()
        }
        DupMode::Latest | DupMode::NoId => { // 1x card, lowest rarity then latest printing. Don't store the ID.
            Regex::new(r"^(\d+ )?([^0-9].*) \(.*\)$").unwrap()
        }
    };
    let scryfall_category = Regex::new(r"^\[\w+\]$").unwrap();
    
    file_write.write_all(PRELUDE).unwrap();
    file_write.write(b"\n").unwrap();
    for line in read2.lines(){
        let line = line.unwrap();
        if scryfall_category.is_match(&line){ //a rarity definition from my scryfall parsing
            file_write.write(line.as_bytes()).unwrap();
            file_write.write(b"\n").unwrap();
            continue;
        }
        let decoded = deunicode(line.trim());
        if let Some(caps) = scryfall_short_card_regex.captures(&decoded){
            let shortname = &caps[2];
            if xmage.contains(shortname){
                file_write.write(line.as_bytes()).unwrap();
                file_write.write(b"\n").unwrap();
            } else {
                println!("not in xmage: {}",line);
            }
            scryfall.insert(shortname.to_string());
        } else {
            println!("could not parse: {}",line);
        }
    }
    //xmage = xmage.sub(&half_cards);
    /*fn check_expand<'a>(str: &'a str, maps: &'a HashMap<String, String>) -> &'a str{
        if let Some(x) = maps.get(str){
            return &x;
        }
        return str;
    }*/


    let diff_out = "differences_output.txt";
    let mut diff_write =  BufWriter::new(File::create(Path::new(diff_out)).unwrap());
    diff_write.write("---XMAGE ONLY---".as_bytes()).unwrap();
    diff_write.write("\n".as_bytes()).unwrap();
    let xmage_only = xmage.difference(&scryfall);
    let mut xmage_only:Vec<_> = xmage_only.into_iter().collect();
    xmage_only.sort();
    for item in xmage_only{
        diff_write.write(item.as_bytes()).unwrap();
        diff_write.write("\n".as_bytes()).unwrap();

    }
    diff_write.write("---SCRYFALL ONLY---".as_bytes()).unwrap();
    diff_write.write("\n".as_bytes()).unwrap();
    let scryfall_only = scryfall.difference(&xmage);
    let mut scryfall_only:Vec<_> = scryfall_only.into_iter().collect();
    scryfall_only.sort();
    for item in scryfall_only{
        diff_write.write(item.as_bytes()).unwrap();
        diff_write.write("\n".as_bytes()).unwrap();
    }
}

impl PartialOrd<CardData> for CardData{
    fn partial_cmp(&self, other: &CardData) -> Option<std::cmp::Ordering>{
        Some(
            self.rarity.cmp(&other.rarity).reverse() //lower rarity
            .then(self.count.cmp(&other.count)) //then higher count
            .then(self.date.cmp(&other.date)) //then later date
        )
    }
}

impl SetData{
    fn insert(&mut self, cardname:String, card: CardData){
        if let Some(oldcard) = self.datamap.get(&cardname){
            if &card > oldcard{
                self.datamap.insert(cardname, card);
            }
        } else {
            self.datamap.insert(cardname, card);
        }
    }
    fn new() -> SetData{
        return SetData{datamap:BTreeMap::new()};
    }
    fn foreach(&self, rarity: Rarity, mut fun:impl FnMut(&String, i32)){
        for (fullname, multiplier) in self.datamap.iter().filter_map(
            |(_cardname, card)| if card.rarity == rarity {Some((&card.name, card.count))} else {None})
        {
            fun(&fullname, multiplier);
        }
    }
    fn filter(&mut self, map: &HashSet<String>){
        self.datamap.retain(|k, _| !map.contains(k))
    }
}

fn make_card_data(card: &Value, rarity: Rarity, canon_name: &HashMap<String, String>) -> (String, CardData){
    let name = get_simplename(card);
    let mut date = card["released_at"].as_str().unwrap().to_string();
    let set = card["set"].as_str().unwrap().to_ascii_uppercase();
    if set == "SLD" {date = "!!!!".to_string()}; //try not to use SLD when possible
    match DUPLICATE_MODE{
        DupMode::Canonicalize => { // 1x card, the oracle_cards version
            return (name.to_string(), CardData{name: canon_name.get(name).unwrap().to_string(), rarity, date, count: get_mult(card)})
        }
        DupMode::Latest => { // 1x card, lowest rarity then latest printing. Don't store the ID.
            let fullname = format!("{} ({})",name, set);
            return (name.to_string(), CardData{name: fullname.to_string(), rarity, date, count: get_mult(card)})
        }
        DupMode::PerSet => { // 1x card per set
            let id = card["collector_number"].as_str().unwrap();
            let fullname = format!("{} ({}) {}",name, set, id);
            let mapname = format!("{} ({})",name, set);
            return (mapname, CardData{name: fullname.to_string(), rarity, date, count: get_mult(card)})
        }
        DupMode::NoId => { // 1x card per set, don't store the ID
            let fullname = format!("{} ({})",name, set);
            return (fullname.to_string(), CardData{name: fullname.to_string(), rarity, date, count: get_mult(card)})
        }
        DupMode::All => { // Every single card printing is separate
            let id = card["collector_number"].as_str().unwrap();
            let fullname = format!("{} ({}) {}",name, set, id);
            return (name.to_string(), CardData{name: fullname.to_string(), rarity, date, count: get_mult(card)})
        }
    }
}