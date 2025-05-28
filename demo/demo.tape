Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 900
Set Shell fish
Set WindowBar Colorful
Set Margin 5
Set MarginFill "#674EFF"
Set BorderRadius 5


Type "bat docker-compose.yml"
Sleep 0.2
Enter

Sleep 0.5

Type "bat html/index.html"
Sleep 0.2
Enter

Sleep 0.5

Type "dcd --version"
Sleep 0.2
Enter


Type "curl 45.76.176.232"
Sleep 0.2
Enter

Sleep 1.2

Type "dcd -i ~/.ssh/id_ed25519 up root@45.76.176.232:22"
Sleep 0.2
Enter
Sleep 20

Type "curl 45.76.176.232"
Sleep 0.2
Enter

Sleep 3s
