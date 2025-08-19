DOTS := $(shell find . -name '*.dot')

.PHONY: all

all: $(DOTS:.dot=.png)

%.png: %.dot
	dot -Tpng $< -o $@
