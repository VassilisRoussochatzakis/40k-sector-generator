mod worlds;

fn main() {
     let xlsx = "M42 Sector Generator.xlsx";
    match worlds::KeyTables::from_xlsx(xlsx) {
        Ok(tables) => {
             println!("Key tables loaded from {xlsx}:");
             println!("  Star colours:   {}", tables.star_colours.len());
             println!("  World types:     {}", tables.world_types.len());
             println!("  Atmospheres:     {}", tables.atmospheres.len());
             println!("  Temperatures:    {}", tables.temperatures.len());
             println!("  Biospheres:      {}", tables.biospheres.len());
             println!("  Populations:     {}", tables.populations.len());
             println!("  Tech levels:     {}", tables.tech_levels.len());
             println!("  Governments:     {}", tables.governments.len());
             println!("  Notable features:{}", tables.notable_features.len());
         },
        Err(e) => eprintln!("Failed: {e}"),
    }
}
