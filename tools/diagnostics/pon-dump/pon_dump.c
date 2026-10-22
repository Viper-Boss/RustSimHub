// SPDX-License-Identifier: GPL-2.0
/*
 * Experimental PM8916 PON diagnostic, based on the source supplied by the
 * user on 2026-10-23. No boot integration or permission to load is implied.
 * Register reads have NOT been verified harmless on the actual board.
 * A device reference does not serialize driver unbind or pin devres regmaps.
 */
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/regmap.h>

static const struct {
	u16 off;
	const char *name;
} pon_regs[] = {
	{ 0x01, "REVISION2" },
	{ 0x05, "SUBTYPE" },
	{ 0x08, "PON_REASON1" },
	{ 0x0a, "WARM_RESET_REASON1" },
	{ 0x0b, "WARM_RESET_REASON2" },
	{ 0x0c, "POFF_REASON1" },
	{ 0x0d, "POFF_REASON2" },
	/* 0x0e is omitted: only a debug-list mention was supplied as evidence. */
	{ 0x5a, "PS_HOLD_RST_CTL" },
	{ 0x5b, "PS_HOLD_RST_CTL2" },
};

static int __init pon_dump_init(void)
{
	struct device_node *np;
	struct platform_device *pdev;
	struct regmap *map;
	u32 base, val;
	unsigned int i;
	int ret, first_error = 0;

	np = of_find_compatible_node(NULL, NULL, "qcom,pm8916-pon");
	if (!np)
		return -ENODEV;

	if (!of_device_is_available(np)) {
		of_node_put(np);
		return -ENODEV;
	}
	ret = of_property_read_u32(np, "reg", &base);
	if (ret) {
		of_node_put(np);
		return ret;
	}
	/* The SPMI regmap uses a 16-bit register address. */
	if (base > 0xffffU - 0x5bU) {
		of_node_put(np);
		return -ERANGE;
	}
	pdev = of_find_device_by_node(np);
	of_node_put(np);
	if (!pdev)
		return -ENODEV;

	map = pdev->dev.parent ? dev_get_regmap(pdev->dev.parent, NULL) : NULL;
	if (!map) {
		put_device(&pdev->dev);
		return -ENODEV;
	}

	pr_info("pon_dump: experimental PON base 0x%03x\n", base);
	for (i = 0; i < ARRAY_SIZE(pon_regs); i++) {
		ret = regmap_read(map, base + pon_regs[i].off, &val);
		if (ret) {
			if (!first_error)
				first_error = ret;
			pr_info("pon_dump: %-20s [0x%03x] read failed: %d\n",
				pon_regs[i].name, base + pon_regs[i].off, ret);
		} else {
			pr_info("pon_dump: %-20s [0x%03x] = 0x%02x\n",
				pon_regs[i].name, base + pon_regs[i].off, val);
		}
	}

	put_device(&pdev->dev);
	return first_error;
}

static void __exit pon_dump_exit(void)
{
}

module_init(pon_dump_init);
module_exit(pon_dump_exit);

MODULE_DESCRIPTION("Experimental PM8916 PON register diagnostic, not hardware validated");
MODULE_LICENSE("GPL");
