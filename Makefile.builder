RPM_SPEC_FILES.dom0 := rpm_spec/dom0.spec
#RPM_SPEC_FILES.vm := rpm_spec/vm.spec
RPM_SPEC_FILES := $(RPM_SPEC_FILES.$(PACKAGE_SET))
#DEBIAN_BUILD_DIRS := debian
ARCH_BUILD_DIRS := archlinux
