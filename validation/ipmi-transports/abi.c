#include <linux/ipmi.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/ioctl.h>

int main(void) {
    printf("IPMI_ABI %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %zu %lu %lu %lu\n",
           sizeof(struct ipmi_msg), sizeof(struct ipmi_req), sizeof(struct ipmi_recv),
           sizeof(struct ipmi_addr), sizeof(struct ipmi_system_interface_addr), sizeof(struct ipmi_ipmb_addr),
           offsetof(struct ipmi_msg, data_len), offsetof(struct ipmi_msg, data),
           offsetof(struct ipmi_req, addr_len), offsetof(struct ipmi_req, msgid), offsetof(struct ipmi_req, msg),
           offsetof(struct ipmi_recv, addr), offsetof(struct ipmi_recv, addr_len), offsetof(struct ipmi_recv, msgid), offsetof(struct ipmi_recv, msg),
           offsetof(struct ipmi_addr, data), offsetof(struct ipmi_system_interface_addr, lun), offsetof(struct ipmi_ipmb_addr, slave_addr),
           (unsigned long)IPMICTL_SEND_COMMAND, (unsigned long)IPMICTL_RECEIVE_MSG_TRUNC, (unsigned long)IPMICTL_GET_MY_ADDRESS_CMD);
    return 0;
}
