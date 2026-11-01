# Make the spec's two sources and a source RPM:
#   make -f packaging/rpm/Makefile srpm outdir=DIR
# COPR's "make srpm" method runs this from the checkout, with the network on,
# so cargo can fetch the crates to vendor.

spec := $(dir $(lastword $(MAKEFILE_LIST)))ginrose.spec
top := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))../..)
version := $(shell sed -n 's/^version = "\(.*\)"/\1/p' $(top)/Cargo.toml | head -n1)
outdir ?= $(top)/dist
work := $(top)/target/rpm-sources

srpm:
	rm -rf $(work) && mkdir -p $(work) $(outdir)
	git -C $(top) archive --format=tar.gz --prefix=ginrose-$(version)/ \
		-o $(work)/ginrose-$(version).tar.gz HEAD
	cd $(top) && cargo vendor --locked --versioned-dirs $(work)/vendor > /dev/null
	tar -C $(work) --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 \
		-cJf $(work)/ginrose-$(version)-vendor.tar.xz vendor
	rpmbuild -bs $(spec) --define "_sourcedir $(work)" --define "_srcrpmdir $(outdir)"

.PHONY: srpm
