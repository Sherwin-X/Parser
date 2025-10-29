# include <stdio.h>
int main() {
    // line comment
    /* nested /* inner */ still ok */
    float f = 1.0e-3f;
    int hex = 0xDEAD_beef;
    char c = '\n';
    char u = '\u{1F600}';
    char hx = '\x41';
    printf("hello, world! %d\n", 42);
    return 0;
}
