clean:
    sudo rm -rf target

update-elf:
    cd program && cargo prove build --elf-name sp1-helios-elf --docker --output-directory ../elf