fun describir(animal: Animal): String {
    val tipo = animal::class.simpleName ?: "Animal"
    return "${animal.nombre} es un $tipo"
}

fun saludar(nombre: String) {
    println("Bienvenido al zoologico, $nombre")
}
