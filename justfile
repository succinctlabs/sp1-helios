clean:
    sudo rm -rf target

update-elf:
    cd program && cargo prove build --docker --output-directory ../elf