#! /usr/bin/env bash

kver=$(cd /usr/lib/modules && echo *)
dracut -vf /usr/lib/modules/$kver/initramfs.img $kver
