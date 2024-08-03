**Magic: The Gathering Chaos Extractor**

This program will take the Scryfall json (https://scryfall.com/docs/api/bulk-data) and the XMage sets directory (https://github.com/magefree/mage/tree/master/Mage.Sets/src/mage/sets but you'll want whatever version your server is running) to create most of a Draftmancer file.

Place the XMage directory and the both the Scryfall Oracle Cards and Default Cards downloads inside the `/src` folder. Remove the date from the names of the Scryfall `.json` files. Then `cargo run` from the base directory of the project.

This will create three files. Once you've done one run, you can disable the `mage_compatible()` call from `main()` since that function's output will be stored in the `EVERYTHING_mage.txt` file. Re-run it and re-copy the set folder when you update XMage to have the latest cards.