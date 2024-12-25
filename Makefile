all: 
	@cd os && mv cargo .cargo
	@cd user && mv cargo .cargo
	@cd os && make run
	@cp ./os/target/riscv64gc-unknown-none-elf/release/os kernel-qemu
	@cp ./bootloader/rustsbi-qemu.bin sbi-qemu	
	@qemu-system-riscv64 \
					-machine virt \
					-m 128M -nographic -smp 2 \
					-bios sbi-qemu \
					-kernel kernel-qemu \
					-drive file=sdcard-riscv.img,if=none,format=raw,id=x0 \
					-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
					-device virtio-net-device,netdev=net -netdev user,id=net	


clean:
	cd os && mv .cargo cargo
	cd user && mv .cargo cargo
	cd os && make clean
	cd modify-img && cargo clean
	cd fat32 && cargo clean
	cd user && make clean
	rm -f kernel-qemu
	rm -f sbi-qemu