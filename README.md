
# Instructions
1. No-Op:     0000 xxxx xxxx xxxx
2. Halt:      0001 xxxx xxxx xxxx - AAAA AAAA AAAA AAAA
3. Load:      0010 AaBb PBxx xxxx - AAAA AAAA AAAA AAAA
4. LoadIA:    0011 OOOO OOOO OOOO
5. LoadIB:    0100 OOOO OOOO OOOO
6. Store:     0101 AaBb Pxxx xxxx - AAAA AAAA AAAA AAAA
7. Op:        0110 xSSS AOXN OOOO
8. Out:       0111 AaBb PBxx xxxx
                   AaBb P

11. Push:     1010 AaBb PBxx xxxx
12. Pop:      1011 AaBb PBxx xxxx
13. Jump:     1100 NnCc ZzEe Nnxx - AAAA AAAA AAAA AAAA
14. 

15. Call:     1110 xxxx xxxx xxxx - AAAA AAAA AAAA AAAA
16. Ret:      1111 xxxx xxxx xxxx
